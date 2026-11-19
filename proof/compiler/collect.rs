//! Emit crate identities, expanded path references and typed call/flow observations
//! while rustc builds MIR. The driver never reads product names or a wiring file.
#![feature(rustc_private)]
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;
extern crate serde_json;

use rustc_data_structures::steal::Steal;
use rustc_hir::{
    self as hir,
    def::DefKind,
    def_id::{DefId, LOCAL_CRATE, LocalDefId},
    intravisit::{self, Visitor},
};
use rustc_interface::interface;
use rustc_middle::{
    hir::nested_filter,
    mir::*,
    ty::{self, Ty, TyCtxt},
};
use rustc_span::{Span, sym};
use serde_json::{Value, json};
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    sync::{Mutex, OnceLock},
};

type Built = for<'tcx> fn(TyCtxt<'tcx>, LocalDefId) -> &'tcx Steal<Body<'tcx>>;
static ORIGINAL: OnceLock<Built> = OnceLock::new();
static STREAM: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

fn emit(tcx: TyCtxt<'_>, record: Value) {
    let mut stream = STREAM
        .get_or_init(|| {
            let name = format!(
                "{}-{:?}.jsonl",
                tcx.crate_name(LOCAL_CRATE),
                tcx.stable_crate_id(LOCAL_CRATE)
            );
            Mutex::new(BufWriter::new(
                File::create(
                    std::path::Path::new(&env::var_os("MAPPER_FACTS").unwrap()).join(name),
                )
                .unwrap(),
            ))
        })
        .lock()
        .unwrap();
    serde_json::to_writer(&mut *stream, &record).unwrap();
    writeln!(&mut *stream).unwrap();
}

fn item(tcx: TyCtxt<'_>, id: DefId) -> Value {
    json!({"id": format!("{:?}",tcx.def_path_hash(id)), "crate":tcx.crate_name(id.krate).to_string(),"unit":format!("{:?}",tcx.stable_crate_id(id.krate)), "name":tcx.def_path_str(id)})
}
fn at(tcx: TyCtxt<'_>, span: Span) -> String {
    tcx.sess.source_map().span_to_diagnostic_string(span)
}

fn place(p: Place<'_>) -> Value {
    let mut fields = vec![];
    let mut indirect = false;
    for projection in p.projection {
        match projection {
            ProjectionElem::Deref => indirect = true,
            ProjectionElem::Field(index, _) => fields.push(json!(index.as_usize())),
            ProjectionElem::Downcast(_, variant) => {
                fields.push(json!(format!("variant:{}", variant.as_usize())))
            }
            _ => return json!({"local":p.local.as_usize(), "opaque":true}),
        }
    }
    json!({"local":p.local.as_usize(),"fields":fields,"indirect":indirect})
}

// Peel shared references and matching smart-pointer constructors. Accept precisely
// one changed type argument. The concrete leaf is kept separate from its holder.
fn erasure<'tcx>(mut from: Ty<'tcx>, mut to: Ty<'tcx>) -> Option<(Ty<'tcx>, DefId)> {
    loop {
        match (*from.kind(), *to.kind()) {
            (ty::Ref(_, a, _), ty::Ref(_, b, _)) => {
                from = a;
                to = b;
            }
            (ty::Adt(..) | ty::Param(_), ty::Dynamic(preds, _)) => {
                return Some((from, preds.principal()?.def_id()));
            }
            (ty::Adt(a, aa), ty::Adt(b, ba)) if a == b => {
                let mut changed = aa.types().zip(ba.types()).filter(|(x, y)| x != y);
                (from, to) = changed.next()?;
                if changed.next().is_some() {
                    return None;
                }
            }
            _ => return None,
        }
    }
}

fn collect<'tcx>(tcx: TyCtxt<'tcx>, def: LocalDefId) -> &'tcx Steal<Body<'tcx>> {
    let built = ORIGINAL.get().unwrap()(tcx, def);
    let body = built.borrow();
    let mut ops = vec![];
    let mut exposed = std::collections::BTreeSet::new();
    let mut predecessors = vec![Vec::new(); body.basic_blocks.len()];
    let mut returns = vec![];
    let mut switches = vec![];
    let mut enum_switches = vec![];
    let labels: std::collections::BTreeMap<_, _> = body
        .var_debug_info
        .iter()
        .filter_map(|v| match v.value {
            VarDebugInfoContents::Place(p) if p.projection.is_empty() => {
                Some((p.local.as_usize(), v.name.to_string()))
            }
            _ => None,
        })
        .collect();
    let operand = |op: &Operand<'tcx>| op.place().map(place);
    for (bb, block) in body.basic_blocks.iter_enumerated() {
        for stmt in &block.statements {
            if let StatementKind::Assign(assignment) = &stmt.kind {
                let (dst, rhs) = &**assignment;
                if let Rvalue::RawPtr(_, p) = rhs {
                    exposed.insert(p.local.as_usize());
                }
                let borrowed = match rhs {
                    Rvalue::Ref(_, BorrowKind::Mut { .. }, p) => Some(place(*p)),
                    _ => None,
                };
                let flow = match rhs {
                    Rvalue::Use(Operand::Constant(c), _)
                        if c.const_.ty().is_bool() && c.const_.try_to_bool().is_some() =>
                    {
                        json!({"boolean":c.const_.try_to_bool()})
                    }
                    Rvalue::Discriminant(p) => json!({"tagOf":place(*p)}),
                    Rvalue::Use(o, _) => json!({"from":operand(o)}),
                    Rvalue::Ref(_, BorrowKind::Shared, p) => json!({"from":place(*p)}),
                    Rvalue::Cast(
                        CastKind::PointerCoercion(ty::adjustment::PointerCoercion::Unsize, _),
                        op,
                        to,
                    ) => match erasure(op.ty(&body.local_decls, tcx), *to) {
                        Some((provider, contract)) => match *provider.kind() {
                            ty::Adt(a, _) => {
                                json!({"provider":item(tcx,a.did()),"contract":item(tcx,contract)})
                            }
                            ty::Param(p) => {
                                json!({"typeParam":p.index,"contract":item(tcx,contract)})
                            }
                            _ => unreachable!(),
                        },
                        None if tcx.erase_and_anonymize_regions(op.ty(&body.local_decls, tcx))
                            == tcx.erase_and_anonymize_regions(*to) =>
                        {
                            json!({"from":operand(op)})
                        }
                        None => json!({"unknown":"unsizing with no concrete nominal leaf"}),
                    },
                    Rvalue::UnaryOp(UnOp::Not, op) if op.ty(&body.local_decls, tcx).is_bool() => {
                        json!({"negated":operand(op)})
                    }
                    Rvalue::Aggregate(kind, args)
                        if matches!(
                            &**kind,
                            AggregateKind::Closure(..)
                                | AggregateKind::Coroutine(..)
                                | AggregateKind::CoroutineClosure(..)
                        ) =>
                    {
                        let id = match &**kind {
                            AggregateKind::Closure(id, _)
                            | AggregateKind::Coroutine(id, _)
                            | AggregateKind::CoroutineClosure(id, _) => *id,
                            _ => unreachable!(),
                        };
                        json!({"environment":item(tcx,id),"parts":args.iter().map(operand).collect::<Vec<_>>()})
                    }
                    Rvalue::Aggregate(kind, args)
                        if let AggregateKind::Adt(id, variant, ..) = &**kind
                            && tcx.adt_def(*id).is_enum() =>
                    {
                        json!({"variant":format!("variant:{}",variant.as_usize()),"parts":args.iter().map(operand).collect::<Vec<_>>()})
                    }
                    Rvalue::Aggregate(kind, args)
                        if matches!(&**kind, AggregateKind::Tuple)
                            || matches!(&**kind, AggregateKind::Adt(id, ..) if tcx.adt_def(*id).is_struct()) =>
                    {
                        json!({"parts":args.iter().map(operand).collect::<Vec<_>>()})
                    }
                    _ => json!({"unknown":"unmodelled value"}),
                };
                ops.push(json!({"bb":bb.as_usize(),"at":at(tcx,stmt.source_info.span),"dst":place(*dst),"borrowed":borrowed,"flow":flow}));
            }
        }
        let t = block.terminator();
        // Built MIR also has imaginary edges for borrow checking a match.
        // Only the real successor represents a possible transfer of control.
        match t.kind {
            TerminatorKind::FalseEdge { real_target, .. }
            | TerminatorKind::FalseUnwind { real_target, .. } => {
                predecessors[real_target.as_usize()].push(bb.as_usize())
            }
            _ => {
                for successor in t.successors() {
                    predecessors[successor.as_usize()].push(bb.as_usize());
                }
            }
        }
        if matches!(t.kind, TerminatorKind::Return) {
            returns.push(bb.as_usize());
        }
        if let TerminatorKind::SwitchInt { discr, targets } = &t.kind
            && discr.ty(&body.local_decls, tcx).is_bool()
            && let Some(p) = discr.place()
            && p.projection.is_empty()
            && targets.target_for_value(0) != targets.target_for_value(1)
        {
            switches.push(json!({"bb":bb.as_usize(),"local":p.local.as_usize(),"yes":targets.target_for_value(1).as_usize(),
                "no":targets.target_for_value(0).as_usize(),"at":at(tcx,t.source_info.span)}));
        }
        if let TerminatorKind::SwitchInt { discr, targets } = &t.kind
            && let Some(test) = discr.place()
            && test.projection.is_empty()
        {
            let assigned = block.statements.iter().rev().find_map(|s| match &s.kind {
                StatementKind::Assign(a) if a.0 == test => Some(&a.1),
                _ => None,
            });
            if let Some(Rvalue::Discriminant(p)) = assigned
                && let ty::Adt(adt, _) = *p.ty(&body.local_decls, tcx).ty.kind()
                && adt.is_enum()
            {
                let cases:Vec<_>=adt.discriminants(tcx).map(|(variant,disc)|json!({
                    "name":adt.variant(variant).name.to_string(),"to":targets.target_for_value(disc.val).as_usize()
                })).collect();
                enum_switches.push(json!({"bb":bb.as_usize(),"at":at(tcx,t.source_info.span),"enumName":tcx.def_path_str(adt.did()),"cases":cases}));
            }
        }
        let (func, args, dst) = match &t.kind {
            TerminatorKind::Call {
                func,
                args,
                destination,
                ..
            } => (func, args, Some(place(*destination))),
            TerminatorKind::TailCall { func, args, .. } => (func, args, None),
            _ => continue,
        };
        let ty = func.ty(&body.local_decls, tcx);
        let receiver = if let ty::FnDef(id, substs) = *ty.kind()
            && tcx.trait_of_assoc(id).is_some()
        {
            match *substs.type_at(0).kind() {
                ty::Adt(a, _) => Some(item(tcx, a.did())),
                _ => None,
            }
        } else {
            None
        };
        let closure = if let ty::FnDef(id, args) = *ty.kind()
            && let Some(trait_id) = tcx.trait_of_assoc(id)
            && [
                tcx.lang_items().fn_trait(),
                tcx.lang_items().fn_mut_trait(),
                tcx.lang_items().fn_once_trait(),
            ]
            .contains(&Some(trait_id))
            && let ty::Closure(def, _) = *args.type_at(0).kind()
        {
            Some(item(tcx, def))
        } else {
            None
        };
        let mut selected = None;
        let mut bindings = None;
        let (kind, target, contract) = match *ty.kind() {
            ty::FnDef(id, substs) => match tcx.trait_of_assoc(id) {
                Some(tr) => {
                    use ty::TypeVisitableExt;
                    let args = tcx.erase_and_anonymize_regions(substs);
                    let instance = if !args.has_non_region_param()
                        && !args.has_aliases()
                        && !args.has_escaping_bound_vars()
                        && !args.has_non_region_infer()
                    {
                        ty::Instance::try_resolve(
                            tcx,
                            ty::TypingEnv::fully_monomorphized(),
                            id,
                            args,
                        )
                        .ok()
                        .flatten()
                    } else {
                        None
                    };
                    if let Some(instance) = instance
                        && matches!(instance.def, ty::InstanceKind::Item(_))
                    {
                        bindings = Some(instance.args);
                    }
                    let kind = match instance.map(|i| i.def) {
                        Some(ty::InstanceKind::Virtual(..)) => "dynamic",
                        Some(ty::InstanceKind::Item(def)) => {
                            selected = Some(item(tcx, def));
                            "static-trait"
                        }
                        _ => "trait",
                    };
                    (kind, Some(item(tcx, id)), Some(item(tcx, tr)))
                }
                None if tcx.is_foreign_item(id) => ("foreign", Some(item(tcx, id)), None),
                None => {
                    bindings = Some(substs);
                    ("direct", Some(item(tcx, id)), None)
                }
            },
            _ => ("indirect", None, None),
        };
        // Only the standard pointer implementations have these payload-preserving
        // contracts. A user-defined Deref/AsRef/Clone can choose another object.
        let mut pointer_op = None;
        if let ty::FnDef(id, substs) = *ty.kind() {
            let tr = tcx.trait_of_assoc(id);
            let target_type = if tr.is_some() {
                Some(substs.type_at(0))
            } else {
                tcx.inherent_impl_of_assoc(id)
                    .map(|i| tcx.type_of(i).instantiate_identity().skip_norm_wip())
            };
            if let Some(t) = target_type
                && let ty::Adt(a, _) = *t.kind()
            {
                let shared = matches!(tcx.get_diagnostic_name(a.did()), Some(sym::Arc | sym::Rc));
                let name = tcx.opt_item_name(id);
                let same_crate = selected.as_ref().is_some_and(|v| {
                    v["unit"] == format!("{:?}", tcx.stable_crate_id(a.did().krate))
                });
                if (shared || a.is_box())
                    && tr.is_none()
                    && name == Some(sym::new)
                    && id.krate == a.did().krate
                {
                    pointer_op = Some("new");
                } else if (shared || a.is_box()) && same_crate {
                    if tr == tcx.lang_items().deref_trait() && name == Some(sym::deref) {
                        pointer_op = Some("deref");
                    }
                    if tr == tcx.get_diagnostic_item(sym::AsRef) && name == Some(sym::as_ref) {
                        pointer_op = Some("as_ref");
                    }
                    if shared && tr == tcx.lang_items().clone_trait() && name == Some(sym::clone) {
                        pointer_op = Some("clone");
                    }
                }
            }
        }
        let nominal_args: std::collections::BTreeMap<_, _> = bindings
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, arg)| match *arg.as_type()?.kind() {
                ty::Adt(a, _) => Some((i, item(tcx, a.did()))),
                _ => None,
            })
            .collect();
        ops.push(json!({"bb":bb.as_usize(),"pointerOp":pointer_op,"nominalArgs":nominal_args,"at":at(tcx,t.source_info.span),"dst":dst,
            "call":kind,"selected":selected,"closure":closure,"nominalSelf":receiver,"target":target,"contract":contract,"args":args.iter().map(|a|operand(&a.node)).collect::<Vec<_>>() }));
    }
    emit(
        tcx,
        json!({"kind":"body","owner":item(tcx,def.to_def_id()),"argc":body.arg_count,"exposed":exposed,"ops":ops,"labels":labels,"switches":switches,"enumSwitches":enum_switches,"predecessors":predecessors,"returns":returns}),
    );
    drop(body);
    built
}

struct Paths<'tcx>(TyCtxt<'tcx>);
impl<'tcx> Visitor<'tcx> for Paths<'tcx> {
    type NestedFilter = nested_filter::All;
    fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
        self.0
    }
    fn visit_path(&mut self, p: &hir::Path<'tcx>, id: hir::HirId) {
        let tcx = self.0;
        if let Some(target) = p.res.opt_def_id()
            && !target.is_local()
        {
            let owner = id.owner.def_id.to_def_id();
            let kind = if tcx.def_kind(owner) == DefKind::Use {
                if tcx.visibility(owner).is_public() {
                    "public-use"
                } else {
                    "use"
                }
            } else {
                "path"
            };
            emit(
                tcx,
                json!({"kind":kind,"owner":item(tcx,owner),"target":item(tcx,target),"at":at(tcx,p.span)}),
            );
        }
        intravisit::walk_path(self, p);
    }
}

struct Collector;
impl rustc_driver::Callbacks for Collector {
    fn config(&mut self, config: &mut interface::Config) {
        config.override_queries = Some(|_, p| {
            ORIGINAL.set(p.queries.mir_built).unwrap();
            p.queries.mir_built = collect;
        });
    }
    fn after_analysis<'tcx>(
        &mut self,
        _: &interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> rustc_driver::Compilation {
        emit(
            tcx,
            json!({"kind":"crate","package":env::var("CARGO_PKG_NAME").ok(),"version":env::var("CARGO_PKG_VERSION").ok(),
            "owner":item(tcx,LOCAL_CRATE.as_def_id())}),
        );
        let expected: Vec<_> = tcx
            .mir_keys(())
            .iter()
            .map(|d| {
                let mut record = item(tcx, d.to_def_id());
                record["definitionKind"] = json!(format!("{:?}", tcx.def_kind(*d)));
                record
            })
            .collect();
        emit(tcx, json!({"kind":"mir-keys","items":expected}));
        tcx.hir_walk_toplevel_module(&mut Paths(tcx));
        for item_id in tcx.hir_free_items() {
            let id = item_id.owner_id.def_id;
            if matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true }) {
                let tr = tcx
                    .impl_trait_ref(id)
                    .instantiate_identity()
                    .skip_norm_wip();
                let provider = match *tr.self_ty().kind() {
                    ty::Adt(a, _) => Some(item(tcx, a.did())),
                    _ => None,
                };
                let methods: Vec<_> = tcx
                    .associated_items(id)
                    .in_definition_order()
                    .filter_map(|i| {
                        i.trait_item_def_id()
                            .map(|m| json!({"body":item(tcx,i.def_id),"member":item(tcx,m)}))
                    })
                    .collect();
                emit(
                    tcx,
                    json!({"kind":"impl","simpleImpl":tcx.generics_of(tr.def_id).count()==1 && matches!(tr.self_ty().kind(),ty::Adt(_,args) if args.is_empty()),"owner":item(tcx,id.to_def_id()),"provider":provider,"contract":item(tcx,tr.def_id),"methods":methods,"at":at(tcx,tcx.def_span(id))}),
                );
            }
        }
        rustc_driver::Compilation::Continue
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let result =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut Collector));
    if let Some(w) = STREAM.get() {
        w.lock().unwrap().flush().unwrap();
    }
    result
}
