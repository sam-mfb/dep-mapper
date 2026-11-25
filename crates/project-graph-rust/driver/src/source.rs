//! Records from the compiler's private interface: the unit, its items, `use`
//! items, paths that name other crates, and trait impls.

use project_graph_rust_records as r;
use rustc_hir::{
    self as hir, ItemKind as HirItemKind, UseKind,
    def::{DefKind, Res},
    def_id::{DefId, LOCAL_CRATE},
    intravisit::{self, Visitor},
};
use rustc_middle::{
    hir::nested_filter,
    ty::{TyCtxt, print::with_no_trimmed_paths, print::with_no_visible_paths},
};
use rustc_session::config::CrateType;
use rustc_span::Span;

pub fn def(tcx: TyCtxt<'_>, id: DefId) -> r::Def {
    r::Def {
        crate_name: tcx.crate_name(id.krate).to_string(),
        crate_id: crate_id(tcx, id.krate),
        path: tcx.def_path(id).to_string_no_crate_verbose(),
        // The path from the defining crate: not a visible alias such as the
        // `_serde` a derive declares, and not shortened.
        name: with_no_visible_paths!(with_no_trimmed_paths!(tcx.def_path_str(id))),
    }
}

fn crate_id(tcx: TyCtxt<'_>, krate: rustc_span::def_id::CrateNum) -> String {
    format!("{:016x}", tcx.stable_crate_id(krate).as_u64())
}

/// The span the user wrote: for macro output, the macro invocation.
pub fn span(tcx: TyCtxt<'_>, span: Span) -> Option<r::Span> {
    let span = span.source_callsite();
    if span.is_dummy() {
        return None;
    }
    let map = tcx.sess.source_map();
    let lo = map.lookup_byte_offset(span.lo());
    let hi = map.lookup_byte_offset(span.hi());
    Some(r::Span {
        file: lo.sf.name.prefer_local_unconditionally().to_string(),
        lo: lo.pos.0 as usize,
        hi: hi.pos.0 as usize,
    })
}

pub fn collect(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    let mut records = vec![r::Record::Unit(unit(tcx))];
    records.extend(items(tcx));
    records.extend(uses(tcx));
    let mut paths = Paths {
        tcx,
        records: Vec::new(),
    };
    tcx.hir_walk_toplevel_module(&mut paths);
    records.extend(paths.records);
    records.extend(impls(tcx));
    records
}

fn unit(tcx: TyCtxt<'_>) -> r::UnitRecord {
    let opts = &tcx.sess.opts;
    let mut features: Vec<String> = tcx
        .sess
        .config
        .iter()
        .filter(|(name, _)| *name == rustc_span::sym::feature)
        .filter_map(|(_, value)| value.map(|v| v.to_string()))
        .collect();
    features.sort();
    r::UnitRecord {
        package: std::env::var("CARGO_PKG_NAME").unwrap_or_default(),
        crate_name: tcx.crate_name(LOCAL_CRATE).to_string(),
        crate_id: crate_id(tcx, LOCAL_CRATE),
        crate_types: tcx
            .crate_types()
            .iter()
            .map(|t| match t {
                CrateType::Executable => "bin",
                CrateType::ProcMacro => "proc-macro",
                _ => "lib",
            })
            .map(String::from)
            .collect(),
        test: opts.test,
        features,
        target_triple: opts.target_triple.tuple().to_string(),
        externs: opts.externs.iter().map(|(name, _)| name.clone()).collect(),
    }
}

fn item_kind(kind: DefKind) -> Option<r::ItemKind> {
    Some(match kind {
        DefKind::Struct
        | DefKind::Enum
        | DefKind::Union
        | DefKind::Trait
        | DefKind::TraitAlias
        | DefKind::TyAlias => r::ItemKind::Type,
        DefKind::Const { .. } | DefKind::Static { .. } | DefKind::AssocConst { .. } => {
            r::ItemKind::Value
        }
        DefKind::Fn => r::ItemKind::Function,
        DefKind::AssocFn => r::ItemKind::Method,
        DefKind::Mod => r::ItemKind::Module,
        _ => return None,
    })
}

fn items(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    let visible = tcx.effective_visibilities(());
    tcx.hir_crate_items(())
        .definitions()
        .filter_map(|local| {
            let id = local.to_def_id();
            let kind = item_kind(tcx.def_kind(id))?;
            let signature =
                matches!(kind, r::ItemKind::Function | r::ItemKind::Method).then(|| {
                    tcx.fn_sig(id)
                        .instantiate_identity()
                        .skip_norm_wip()
                        .to_string()
                });
            Some(r::Record::Item(r::ItemRecord {
                def: def(tcx, id),
                kind,
                exported: visible.is_exported(local),
                signature,
                span: span(tcx, tcx.def_span(id)),
            }))
        })
        .collect()
}

fn uses(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    let mut records = Vec::new();
    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        let HirItemKind::Use(path, kind) = item.kind else {
            continue;
        };
        let local_name = match kind {
            UseKind::Single(ident) => ident.to_string(),
            UseKind::Glob => "*".to_string(),
            UseKind::ListStem => continue,
        };
        // Whether other crates can name it, not just whether it says `pub`.
        let local = item.owner_id.def_id;
        let Some(at) = span(tcx, item.span) else {
            continue;
        };
        // A unit struct is bound in both namespaces; name it once.
        let mut targets: Vec<DefId> = path
            .res
            .present_items()
            .filter_map(|res| res.opt_def_id())
            .filter(|t| !t.is_local())
            .map(|t| match tcx.def_kind(t) {
                DefKind::Ctor(..) => tcx.parent(t),
                _ => t,
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        targets.retain(|t| seen.insert(*t));
        for target in targets {
            records.push(r::Record::Use(r::UseRecord {
                module: def(tcx, tcx.parent_module_from_def_id(local).to_def_id()),
                target: def(tcx, target),
                local_name: local_name.clone(),
                public: tcx.effective_visibilities(()).is_exported(local),
                span: at.clone(),
            }));
        }
    }
    records
}

struct Paths<'tcx> {
    tcx: TyCtxt<'tcx>,
    records: Vec<r::Record>,
}

impl<'tcx> Visitor<'tcx> for Paths<'tcx> {
    type NestedFilter = nested_filter::All;

    fn maybe_tcx(&mut self) -> Self::MaybeTyCtxt {
        self.tcx
    }

    fn visit_path(&mut self, path: &hir::Path<'tcx>, id: hir::HirId) {
        let tcx = self.tcx;
        let owner = id.owner.def_id.to_def_id();
        if let Res::Def(kind, target) = path.res
            && !target.is_local()
            && tcx.def_kind(owner) != DefKind::Use
            && let Some((kind, target)) = reference(tcx, kind, target)
            && let Some(at) = span(tcx, path.span)
        {
            self.records.push(r::Record::Reference(r::ReferenceRecord {
                owner: def(tcx, owner),
                target: def(tcx, target),
                kind,
                span: at,
            }));
        }
        intravisit::walk_path(self, path);
    }
}

fn reference(tcx: TyCtxt<'_>, kind: DefKind, target: DefId) -> Option<(r::ReferenceKind, DefId)> {
    Some(match kind {
        DefKind::Struct
        | DefKind::Enum
        | DefKind::Union
        | DefKind::Trait
        | DefKind::TraitAlias
        | DefKind::TyAlias
        | DefKind::ForeignTy
        | DefKind::AssocTy => (r::ReferenceKind::Type, target),
        // Name the struct or variant a constructor builds.
        DefKind::Ctor(..) => (r::ReferenceKind::Value, tcx.parent(target)),
        DefKind::Fn
        | DefKind::AssocFn
        | DefKind::Const { .. }
        | DefKind::AssocConst { .. }
        | DefKind::Static { .. }
        | DefKind::Variant => (r::ReferenceKind::Value, target),
        _ => return None,
    })
}

fn impls(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    tcx.hir_free_items()
        .filter_map(|item_id| {
            let id = item_id.owner_id.def_id;
            if !matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true }) {
                return None;
            }
            let contract = tcx
                .impl_trait_ref(id)
                .instantiate_identity()
                .skip_norm_wip();
            let self_type = match *contract.self_ty().kind() {
                rustc_middle::ty::Adt(adt, _) => Some(def(tcx, adt.did())),
                _ => None,
            };
            Some(r::Record::Impl(r::ImplRecord {
                self_type,
                contract: def(tcx, contract.def_id),
                span: span(tcx, tcx.def_span(id))?,
            }))
        })
        .collect()
}
