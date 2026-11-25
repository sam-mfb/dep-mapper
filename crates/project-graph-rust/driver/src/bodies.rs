//! Records from function bodies, read through `rustc_public`.

use crate::source::{def, span};
use project_graph_rust_records as r;
use rustc_middle::ty::{TyCtxt, TypeVisitableExt};
use rustc_public::{
    CrateDef, ItemKind,
    mir::{TerminatorKind, mono::Instance, mono::InstanceKind},
    rustc_internal::{internal, run},
    ty::{RigidTy, TyKind},
};

pub fn collect(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    match run(tcx, || calls(tcx)) {
        Ok(records) => records,
        Err(error) => vec![r::Record::Gap(r::GapRecord {
            span: None,
            reason: format!("function bodies could not be read: {error:?}"),
        })],
    }
}

fn calls(tcx: TyCtxt<'_>) -> Vec<r::Record> {
    let mut records = Vec::new();
    for item in rustc_public::all_local_items() {
        let owner = tcx.typeck_root_def_id(internal(tcx, item.def_id()));
        let Some(body) = item.body() else {
            // Constants and statics without a body hold no calls.
            if item.kind() != ItemKind::Fn {
                continue;
            }
            records.push(r::Record::Gap(r::GapRecord {
                span: span(tcx, tcx.def_span(owner)),
                reason: format!("no body available for {}", tcx.def_path_str(owner)),
            }));
            continue;
        };
        for block in &body.blocks {
            let TerminatorKind::Call { func, .. } = &block.terminator.kind else {
                continue;
            };
            let Some(at) = span(tcx, internal(tcx, block.terminator.span)) else {
                continue;
            };
            let (callee, dispatch) = match func.ty(body.locals()).map(|t| t.kind()) {
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
                            Ok(instance)
                                if matches!(instance.kind, InstanceKind::Virtual { .. }) =>
                            {
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
            };
            records.push(r::Record::Call(r::CallRecord {
                owner: def(tcx, owner),
                callee: callee.map(|c| def(tcx, c)),
                dispatch,
                span: at,
            }));
        }
    }
    records
}
