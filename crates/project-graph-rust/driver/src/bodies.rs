//! Records from function bodies, read through `rustc_public`: calls, value
//! moves, and the receivers of calls left at a trait.

use crate::source::{def, span};
use project_graph_rust_records as r;
use rustc_middle::ty::{TyCtxt, TypeVisitableExt};
use rustc_public::{
    CrateDef, CrateItem, ItemKind,
    mir::{
        AggregateKind, Body, CastKind, Operand, Place, PointerCoercion, ProjectionElem, Rvalue,
        StatementKind, TerminatorKind, VarDebugInfoContents,
        mono::{Instance, InstanceKind},
    },
    rustc_internal::{internal, run},
    ty::{AdtKind, RigidTy, Ty, TyKind},
};

pub fn collect(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    match run(tcx, || {
        rustc_public::all_local_items()
            .into_iter()
            .flat_map(|item| BodyReader::read(tcx, item))
            .collect()
    }) {
        Ok(records) => records,
        Err(error) => vec![r::Record::Gap(r::GapRecord {
            span: None,
            reason: format!("function bodies could not be read: {error:?}"),
        })],
    }
}

struct BodyReader<'tcx> {
    tcx: TyCtxt<'tcx>,
    /// The function, or closure, whose body this is.
    function: r::Def,
    /// The item that contains it: calls are reported under this.
    owner: r::Def,
    body: Body,
    names: Vec<Option<String>>,
    records: Vec<r::Record>,
}

impl<'tcx> BodyReader<'tcx> {
    fn read(tcx: TyCtxt<'tcx>, item: CrateItem) -> Vec<r::Record> {
        let id = internal(tcx, item.def_id());
        let owner = tcx.typeck_root_def_id(id);
        let Some(body) = item.body() else {
            // Constants and statics without a body hold no calls.
            if item.kind() != ItemKind::Fn {
                return Vec::new();
            }
            return vec![r::Record::Gap(r::GapRecord {
                span: span(tcx, tcx.def_span(owner)),
                reason: format!("no body available for {}", tcx.def_path_str(id)),
            })];
        };
        let mut names = vec![None; body.locals().len()];
        for info in &body.var_debug_info {
            if let VarDebugInfoContents::Place(place) = &info.value
                && place.projection.is_empty()
                && info.composite.is_none()
            {
                names[place.local] = Some(info.name.clone());
            }
        }
        let mut reader = BodyReader {
            tcx,
            function: def(tcx, id),
            owner: def(tcx, owner),
            body,
            names,
            records: Vec::new(),
        };
        reader.statements();
        reader.terminators();
        reader.records
    }

    fn span(&self, at: rustc_public::ty::Span) -> Option<r::Span> {
        span(self.tcx, internal(self.tcx, at))
    }

    fn def(&self, id: rustc_public::DefId) -> r::Def {
        def(self.tcx, internal(self.tcx, id))
    }

    /// Where a place's value is held. A place inside a struct field or a
    /// closure capture is that field or capture; otherwise it is the local.
    fn node(&self, place: &Place) -> r::Node {
        let mut ty = self.body.locals()[place.local].ty;
        let mut field = None;
        for projection in &place.projection {
            match projection {
                // Inside the standard library's own types, such as a box's
                // pointer, the value stays attributed to what holds it.
                ProjectionElem::Field(_, field_ty) if self.in_std(ty) => ty = *field_ty,
                ProjectionElem::Field(index, field_ty) => {
                    field = match ty.kind() {
                        TyKind::RigidTy(RigidTy::Adt(adt, _)) if adt.kind() == AdtKind::Struct => {
                            adt.variants()
                                .first()
                                .and_then(|v| v.fields().get(*index).map(|f| f.name.clone()))
                                .map(|name| r::Node::Field {
                                    owner: self.def(adt.def_id()),
                                    field: name,
                                })
                        }
                        TyKind::RigidTy(RigidTy::Closure(closure, _)) => Some(r::Node::Capture {
                            closure: self.def(closure.def_id()),
                            index: *index as u32,
                        }),
                        // An async block or `async fn` body holds what it
                        // captured as fields of its coroutine.
                        TyKind::RigidTy(RigidTy::Coroutine(coroutine, _)) => {
                            Some(r::Node::Capture {
                                closure: self.def(coroutine.def_id()),
                                index: *index as u32,
                            })
                        }
                        TyKind::RigidTy(RigidTy::CoroutineClosure(closure, _)) => {
                            Some(r::Node::Capture {
                                closure: self.def(closure.def_id()),
                                index: *index as u32,
                            })
                        }
                        _ => None,
                    };
                    ty = *field_ty;
                }
                ProjectionElem::Deref => match deref(ty) {
                    Some(inner) => ty = inner,
                    None => break,
                },
                // Enums, indexes and the like are not followed: the value is
                // attributed to the field or local that holds it.
                _ => break,
            }
        }
        field.unwrap_or_else(|| self.local(place.local))
    }

    fn in_std(&self, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::RigidTy(RigidTy::Adt(adt, _)) => {
                let krate = internal(self.tcx, adt.def_id()).krate;
                matches!(
                    self.tcx.crate_name(krate).as_str(),
                    "core" | "alloc" | "std"
                )
            }
            _ => false,
        }
    }

    fn local(&self, local: usize) -> r::Node {
        let function = self.function.clone();
        if local == 0 {
            r::Node::Returned { function }
        } else if local <= self.body.arg_locals().len() {
            r::Node::Param {
                function,
                index: (local - 1) as u32,
            }
        } else {
            r::Node::Local {
                function,
                local: local as u32,
                name: self.names[local].clone(),
            }
        }
    }

    fn type_node(&self, ty: Ty) -> Option<r::Node> {
        match ty.kind() {
            TyKind::RigidTy(RigidTy::Adt(adt, _)) => Some(r::Node::Type(self.def(adt.def_id()))),
            TyKind::RigidTy(RigidTy::Closure(closure, _)) => {
                Some(r::Node::Type(self.def(closure.def_id())))
            }
            _ => None,
        }
    }

    /// The node an operand reads, or for a constant of a named type, the
    /// construction of that type.
    fn operand(&self, operand: &Operand) -> Option<(r::Node, r::FlowKind)> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                Some((self.node(place), r::FlowKind::Copy))
            }
            Operand::Constant(constant) => self
                .type_node(constant.ty())
                .map(|t| (t, r::FlowKind::Construct)),
            _ => None,
        }
    }

    fn flow(
        &mut self,
        from: r::Node,
        to: r::Node,
        kind: r::FlowKind,
        call_site: Option<String>,
        at: Option<r::Span>,
    ) {
        // A copy into or out of a field or capture says so.
        let kind = match (&from, &to, kind) {
            (_, r::Node::Field { .. }, r::FlowKind::Copy) => r::FlowKind::Store,
            (_, r::Node::Capture { .. }, r::FlowKind::Copy) => r::FlowKind::Capture,
            (r::Node::Field { .. } | r::Node::Capture { .. }, _, r::FlowKind::Copy) => {
                r::FlowKind::Read
            }
            _ => kind,
        };
        if from != to {
            self.records.push(r::Record::Flow(r::FlowRecord {
                from,
                to,
                kind,
                call_site,
                span: at,
            }));
        }
    }

    fn statements(&mut self) {
        let blocks = self.body.blocks.clone();
        for block in &blocks {
            for statement in &block.statements {
                let StatementKind::Assign(place, value) = &statement.kind else {
                    continue;
                };
                let at = self.span(statement.span);
                let to = self.node(place);
                match value {
                    Rvalue::Use(operand, _)
                    | Rvalue::Cast(CastKind::PtrToPtr | CastKind::Transmute, operand, _) => {
                        if let Some((from, kind)) = self.operand(operand) {
                            self.flow(from, to, kind, None, at);
                        }
                    }
                    Rvalue::Ref(_, _, from)
                    | Rvalue::AddressOf(_, from)
                    | Rvalue::CopyForDeref(from) => {
                        let from = self.node(from);
                        self.flow(from, to, r::FlowKind::Copy, None, at);
                    }
                    Rvalue::Cast(
                        CastKind::PointerCoercion(PointerCoercion::Unsize),
                        operand,
                        target,
                    ) => {
                        let Ok(source) = operand.ty(self.body.locals()) else {
                            continue;
                        };
                        match erased(source, *target).and_then(|leaf| self.type_node(leaf)) {
                            Some(provider) => {
                                self.flow(provider, to, r::FlowKind::Coerce, None, at)
                            }
                            None => {
                                if let Some((from, kind)) = self.operand(operand) {
                                    self.flow(from, to, kind, None, at);
                                }
                            }
                        }
                    }
                    Rvalue::Aggregate(kind, operands) => self.aggregate(kind, operands, to, at),
                    _ => {}
                }
            }
        }
    }

    fn aggregate(
        &mut self,
        kind: &AggregateKind,
        operands: &[Operand],
        to: r::Node,
        at: Option<r::Span>,
    ) {
        match kind {
            AggregateKind::Adt(adt, variant, ..) => {
                let adt_def = self.def(adt.def_id());
                if adt.kind() == AdtKind::Struct
                    && let Some(fields) = adt.variant(*variant).map(|v| v.fields())
                {
                    for (operand, field) in operands.iter().zip(fields) {
                        if let Some((from, kind)) = self.operand(operand) {
                            let field = r::Node::Field {
                                owner: adt_def.clone(),
                                field: field.name.clone(),
                            };
                            self.flow(from, field, kind, None, at.clone());
                        }
                    }
                }
                self.flow(r::Node::Type(adt_def), to, r::FlowKind::Construct, None, at);
            }
            AggregateKind::Closure(..)
            | AggregateKind::Coroutine(..)
            | AggregateKind::CoroutineClosure(..) => {
                let closure_def = match kind {
                    AggregateKind::Closure(closure, _) => self.def(closure.def_id()),
                    AggregateKind::Coroutine(coroutine, _) => self.def(coroutine.def_id()),
                    AggregateKind::CoroutineClosure(closure, _) => self.def(closure.def_id()),
                    _ => unreachable!("matched above"),
                };
                for (index, operand) in operands.iter().enumerate() {
                    if let Some((from, kind)) = self.operand(operand) {
                        let capture = r::Node::Capture {
                            closure: closure_def.clone(),
                            index: index as u32,
                        };
                        self.flow(from, capture, kind, None, at.clone());
                    }
                }
                self.flow(
                    r::Node::Type(closure_def),
                    to,
                    r::FlowKind::Construct,
                    None,
                    at,
                );
            }
            // Tuples and arrays are not followed element by element.
            _ => {
                for operand in operands {
                    if let Some((from, kind)) = self.operand(operand) {
                        self.flow(from, to.clone(), kind, None, at.clone());
                    }
                }
            }
        }
    }

    fn terminators(&mut self) {
        let blocks = self.body.blocks.clone();
        for (index, block) in blocks.iter().enumerate() {
            let TerminatorKind::Call {
                func,
                args,
                destination,
                ..
            } = &block.terminator.kind
            else {
                continue;
            };
            let Some(at) = self.span(block.terminator.span) else {
                continue;
            };
            let (callee, dispatch) = self.callee(func);
            let call_site = format!("{}{}#{index}", self.function.crate_id, self.function.path);
            if let Some(callee) = callee {
                let callee_def = def(self.tcx, callee);
                for (position, arg) in args.iter().enumerate() {
                    if let Some((from, kind)) = self.operand(arg) {
                        let param = r::Node::Param {
                            function: callee_def.clone(),
                            index: position as u32,
                        };
                        let kind = match kind {
                            r::FlowKind::Copy => r::FlowKind::Pass,
                            other => other,
                        };
                        self.flow(from, param, kind, Some(call_site.clone()), Some(at.clone()));
                    }
                }
                let to = self.node(destination);
                self.flow(
                    r::Node::Returned {
                        function: callee_def.clone(),
                    },
                    to,
                    r::FlowKind::Return,
                    Some(call_site.clone()),
                    Some(at.clone()),
                );
                // Only a method's first argument is its receiver; a trait
                // function without `self`, such as `T::build(n)`, has none.
                if dispatch == r::Dispatch::Contract
                    && self.tcx.associated_item(callee).is_method()
                    && let Some(Operand::Copy(place) | Operand::Move(place)) = args.first()
                {
                    let receiver = self.node(place);
                    self.records.push(r::Record::Receiver(r::ReceiverRecord {
                        owner: self.owner.clone(),
                        contract: callee_def,
                        receiver,
                        span: at.clone(),
                    }));
                }
            }
            self.records.push(r::Record::Call(r::CallRecord {
                owner: self.owner.clone(),
                callee: callee.map(|c| def(self.tcx, c)),
                dispatch,
                span: at,
            }));
        }
    }

    fn callee(&self, func: &Operand) -> (Option<rustc_span::def_id::DefId>, r::Dispatch) {
        let tcx = self.tcx;
        match func.ty(self.body.locals()).map(|t| t.kind()) {
            Ok(TyKind::RigidTy(RigidTy::FnDef(callee, args))) => {
                let declared = internal(tcx, callee.def_id());
                if tcx.is_foreign_item(declared) {
                    (Some(declared), r::Dispatch::Foreign)
                } else if tcx.trait_of_assoc(declared).is_none() {
                    (Some(declared), r::Dispatch::Direct)
                } else if internal(tcx, &args).has_param() {
                    (Some(declared), r::Dispatch::Contract)
                } else {
                    match Instance::resolve(callee, &args) {
                        Ok(instance) if instance.kind == InstanceKind::Item => (
                            Some(internal(tcx, instance.def.def_id())),
                            r::Dispatch::SelectedImplementation,
                        ),
                        Ok(instance) if matches!(instance.kind, InstanceKind::Virtual { .. }) => {
                            (Some(declared), r::Dispatch::Contract)
                        }
                        // A compiler-generated implementation, such as a
                        // closure's call operator or a derived method.
                        Ok(_) => (Some(declared), r::Dispatch::SelectedImplementation),
                        Err(_) => (Some(declared), r::Dispatch::Contract),
                    }
                }
            }
            _ => (None, r::Dispatch::Indirect),
        }
    }
}

/// The type a reference, raw pointer or box points to.
fn deref(ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::RigidTy(RigidTy::Ref(_, inner, _) | RigidTy::RawPtr(inner, _)) => Some(inner),
        TyKind::RigidTy(RigidTy::Adt(adt, args)) if adt.is_box() => {
            args.0.first().and_then(|a| a.ty().copied())
        }
        _ => None,
    }
}

/// For an unsizing conversion from `from` to `to`, the concrete type that
/// becomes a trait object: peel matching pointers and the one type argument
/// that differs between the two.
fn erased(mut from: Ty, mut to: Ty) -> Option<Ty> {
    loop {
        match (from.kind(), to.kind()) {
            (
                TyKind::RigidTy(RigidTy::Ref(_, a, _) | RigidTy::RawPtr(a, _)),
                TyKind::RigidTy(RigidTy::Ref(_, b, _) | RigidTy::RawPtr(b, _)),
            ) => {
                from = a;
                to = b;
            }
            (
                TyKind::RigidTy(RigidTy::Adt(..) | RigidTy::Closure(..)),
                TyKind::RigidTy(RigidTy::Dynamic(..)),
            ) => return Some(from),
            (
                TyKind::RigidTy(RigidTy::Adt(a, a_args)),
                TyKind::RigidTy(RigidTy::Adt(b, b_args)),
            ) if a == b => {
                let mut changed = a_args
                    .0
                    .iter()
                    .zip(b_args.0.iter())
                    .filter_map(|(x, y)| Some((*x.ty()?, *y.ty()?)))
                    .filter(|(x, y)| x != y);
                (from, to) = changed.next()?;
                if changed.next().is_some() {
                    return None;
                }
            }
            _ => return None,
        }
    }
}
