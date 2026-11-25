use super::sharsheret;
use crate::*;

fn citations(map: &ProjectMap) -> Vec<&CitationId> {
    fn review<T>(review: &Review<T>) -> Vec<&CitationId> {
        match review {
            Review::Judged {
                primary,
                supporting,
                ..
            } => std::iter::once(primary).chain(supporting).collect(),
            _ => vec![],
        }
    }
    let mut all = vec![];
    for module in map.modules.values() {
        all.extend(review(&module.description));
    }
    for dependency in map.dependencies.values() {
        all.extend(&dependency.evidence);
        all.extend(review(&dependency.description));
    }
    for route in map.routes.values() {
        all.extend(&route.use_site);
        all.extend(route.hops.iter().map(|hop| &hop.evidence));
        if let MapRouteEnd::Stopped { at, .. } = &route.end {
            all.push(at);
        }
        all.extend(review(&route.description));
    }
    all
}

#[test]
fn records_every_module_dependency_and_route_of_the_proposal() {
    let map = sharsheret();
    assert_eq!(map.modules.len(), 11);
    assert_eq!(map.dependencies.len(), 20);
    assert_eq!(map.routes.len(), 11);
}

#[test]
fn every_citation_resolves_to_evidence() {
    let map = sharsheret();
    for citation in citations(&map) {
        assert!(map.evidence.contains_key(citation), "missing {citation:?}");
    }
}

#[test]
fn every_dependency_and_route_joins_modules_on_the_map() {
    let map = sharsheret();
    for id in map.dependencies.keys() {
        assert!(
            map.modules.contains_key(&id.from),
            "unknown module {:?}",
            id.from
        );
        assert!(
            map.modules.contains_key(&id.to),
            "unknown module {:?}",
            id.to
        );
    }
    for route in map.routes.values() {
        let modules = std::iter::once(route.origin.module())
            .chain(route.hops.iter().map(|hop| hop.to.module()));
        for module in modules {
            let module = module.expect("example routes stay in the project");
            assert!(
                map.modules.contains_key(module),
                "unknown module {module:?}"
            );
        }
    }
}

#[test]
fn every_hop_travels_along_a_dependency_on_the_map() {
    let map = sharsheret();
    for (id, route) in &map.routes {
        for index in 0..route.hops.len() {
            let dependency = route
                .hop_dependency(index)
                .unwrap_or_else(|| panic!("{id:?} hop {index} names no dependency"));
            assert!(
                map.dependencies.contains_key(&dependency),
                "{id:?} hop {index} travels along missing {dependency:?}"
            );
        }
    }
}

#[test]
fn every_assigned_layer_is_listed() {
    let map = sharsheret();
    for module in map.modules.values() {
        let Review::Judged { judgment, .. } = &module.description else {
            panic!("{} is unreviewed", module.name)
        };
        let layer = judgment.layer.as_ref().expect("every module has a layer");
        assert!(map.layers.contains(layer), "unknown layer {layer:?}");
    }
}
