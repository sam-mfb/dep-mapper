//! Inventory requested dispatch tables across the Cargo build, including prefix upcasts.
use crate::{emit, item};
use rustc_hir::def_id::LOCAL_CRATE;
use rustc_middle::{
    mir::interpret::AllocId,
    ty::{self, Ty, TyCtxt},
};
use serde_json::json;
use std::sync::OnceLock;

type Allocation =
    for<'tcx> fn(TyCtxt<'tcx>, (Ty<'tcx>, Option<ty::ExistentialTraitRef<'tcx>>)) -> AllocId;
type Partition = for<'tcx> fn(TyCtxt<'tcx>, ()) -> rustc_middle::mono::MonoItemPartitions<'tcx>;
static PARTITION: OnceLock<Partition> = OnceLock::new();
static NAMES: OnceLock<std::collections::BTreeSet<String>> = OnceLock::new();
fn named(tcx: TyCtxt<'_>, id: rustc_hir::def_id::DefId) -> bool {
    NAMES
        .get_or_init(|| {
            std::env::var("MAPPER_TRAIT_CRATES")
                .unwrap()
                .lines()
                .map(String::from)
                .collect()
        })
        .contains(tcx.crate_name(id.krate).as_str())
}
fn partition(tcx: TyCtxt<'_>, key: ()) -> rustc_middle::mono::MonoItemPartitions<'_> {
    let units = PARTITION.get().unwrap()(tcx, key);
    for cgu in units.codegen_units {
        for m in cgu.items().keys() {
            let rustc_middle::mono::MonoItem::Fn(i) = m else {
                continue;
            };
            let ty::InstanceKind::Item(d) = i.def else {
                continue;
            };
            if !named(tcx, d) {
                continue;
            }
            let bindings: std::collections::BTreeMap<_, _> = i
                .args
                .iter()
                .enumerate()
                .filter_map(|(idx, arg)| match *arg.as_type()?.kind() {
                    ty::Adt(a, _) => Some((idx, item(tcx, a.did()))),
                    _ => None,
                })
                .collect();
            if !bindings.is_empty() {
                emit(
                    tcx,
                    json!({"kind":"compiled-bindings","owner":item(tcx,d),"nominalArgs":bindings,
                "emitter":format!("{:?}",tcx.stable_crate_id(LOCAL_CRATE))}),
                );
            }
        }
    }
    units
}
static ORIGINAL: OnceLock<Allocation> = OnceLock::new();

pub fn install(p: &mut rustc_middle::util::Providers) {
    PARTITION
        .set(p.queries.collect_and_partition_mono_items)
        .unwrap();
    p.queries.collect_and_partition_mono_items = partition;
    ORIGINAL.set(p.queries.vtable_allocation).unwrap();
    p.queries.vtable_allocation = allocation;
}

fn allocation<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: (Ty<'tcx>, Option<ty::ExistentialTraitRef<'tcx>>),
) -> AllocId {
    let id = ORIGINAL.get().unwrap()(tcx, key);
    if let (receiver, Some(principal)) = key {
        // A subtrait's table can be used through a prefix without allocating
        // another table. Include every supertrait. Erasing trait arguments here
        // deliberately widens the bound for differently instantiated traits.
        let contracts: Vec<_> = ty::elaborate::supertrait_def_ids(tcx, principal.def_id)
            .map(|d| item(tcx, d))
            .collect();
        let nominal = match *receiver.kind() {
            ty::Adt(d, _) => Some(item(tcx, d.did())),
            _ => None,
        };
        emit(
            tcx,
            json!({"kind":"dispatch-table","emitter":format!("{:?}",tcx.stable_crate_id(LOCAL_CRATE)),"contracts":contracts,"principal":item(tcx,principal.def_id),
            "typeName":receiver.to_string(),"typeId":format!("{:?}",tcx.type_id_hash(receiver)),
            "nominal":nominal}),
        );
    }
    id
}

pub fn manifest(tcx: TyCtxt<'_>) {
    emit(
        tcx,
        json!({"kind":"dispatch-unit","owner":item(tcx,LOCAL_CRATE.as_def_id()),
        "package":std::env::var("CARGO_PKG_NAME").ok(),"manifest":std::env::var("CARGO_MANIFEST_DIR").ok()}),
    );
}

// Query the executable compiler's complete dependency graph, not just local
// rustdoc impls. This envelope also covers calls optimized before table emission.
pub fn implementations(tcx: TyCtxt<'_>) {
    if std::env::var_os("CARGO_BIN_NAME").is_none() {
        return;
    }
    let names = std::env::var("MAPPER_TRAIT_CRATES").unwrap();
    let graph: Vec<_> = std::iter::once(LOCAL_CRATE)
        .chain(tcx.crates(()).iter().copied())
        .map(|c| format!("{:?}", tcx.stable_crate_id(c)))
        .collect();
    let binary = item(tcx, LOCAL_CRATE.as_def_id());
    emit(
        tcx,
        json!({"kind":"executable-graph","binary":binary,"units":graph}),
    );
    for tr in tcx.all_traits_including_private() {
        if !names
            .lines()
            .any(|n| n == tcx.crate_name(tr.krate).as_str())
        {
            continue;
        }
        let mut candidates = vec![];
        for id in tcx.all_impls(tr) {
            let ty = tcx
                .impl_trait_ref(id)
                .instantiate_identity()
                .skip_norm_wip()
                .self_ty();
            let nominal = match *ty.kind() {
                ty::Adt(a, _) => Some(item(tcx, a.did())),
                _ => None,
            };
            let methods: Vec<_> = tcx
                .associated_items(id)
                .in_definition_order()
                .filter_map(|m| {
                    m.trait_item_def_id()
                        .map(|member| json!({"member":item(tcx,member),"body":item(tcx,m.def_id)}))
                })
                .collect();
            candidates.push(json!({"impl":item(tcx,id),"nominal":nominal,"selfPattern":ty.to_string(),"methods":methods}));
        }
        let supers: Vec<_> = ty::elaborate::supertrait_def_ids(tcx, tr)
            .map(|d| item(tcx, d))
            .collect();
        emit(
            tcx,
            json!({"supers":supers,"kind":"impl-envelope","binary":binary,"contract":item(tcx,tr),"candidates":candidates}),
        );
    }
}
