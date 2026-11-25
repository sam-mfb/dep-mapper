use super::*;
use project_map::example::{PROPOSED_ROUTE, sharsheret};

const NOTE: &str = "note:cache";

fn module_id(name: &str) -> ModuleId {
    ModuleId(name.into())
}

fn dependency_id(from: &str, to: &str) -> DependencyId {
    DependencyId {
        from: module_id(from),
        to: module_id(to),
    }
}

fn judged<T>(judgment: T) -> Review<T> {
    Review::Judged {
        judgment,
        reason: "Because.".into(),
        primary: CitationId(NOTE.into()),
        supporting: vec![],
    }
}

/// An empty draft over the saved example map, with one proposal note.
fn draft() -> Draft {
    Draft {
        base: DraftBase::Saved("map.toml".into()),
        layers: Layers::empty(),
        modules: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        routes: BTreeMap::new(),
        evidence: BTreeMap::from([(
            CitationId(NOTE.into()),
            MapEvidence {
                kind: EvidenceKind::Proposal,
                build_unit: None,
                summary: "Keep built maps in memory.".into(),
                location: None,
            },
        )]),
        update: Updates::default(),
        remove: Removals::default(),
    }
}

/// A proposed module in the example's Map layer.
fn cache() -> MapModule {
    MapModule {
        status: Status::ProposedAddition,
        name: "map-cache".into(),
        surface: vec![],
        description: judged(ModuleDescription {
            concern: "Keeps built maps in memory.".into(),
            kind: ModuleKind::Effect,
            layer: Some(LayerName("Map".into())),
        }),
    }
}

fn uses(role: UseRole) -> MapDependency {
    MapDependency {
        status: Status::ProposedAddition,
        evidence: vec![CitationId(NOTE.into())],
        description: judged(RouteDescription {
            role,
            other_roles: vec![],
        }),
    }
}

fn update<C>(changes: C) -> Update<C> {
    Update {
        changes,
        reason: "Rendering becomes pure.".into(),
        primary: CitationId(NOTE.into()),
        supporting: vec![],
    }
}

fn combined(draft: &Draft) -> Combined {
    combine(draft, Some(&sharsheret())).expect("the draft has a base")
}

#[test]
fn additions_join_the_base_map() {
    let mut draft = draft();
    draft.modules.insert(module_id("map-cache"), cache());
    draft.dependencies.insert(
        dependency_id("map-cache", "project-map"),
        uses(UseRole::DirectUse),
    );
    let result = combined(&draft);
    assert_eq!(result.problems, []);
    assert_eq!(result.map.modules[&module_id("map-cache")], cache());
    assert_eq!(result.map.source, sharsheret().source);
    assert_eq!(result.map.modules.len(), sharsheret().modules.len() + 1);
}

#[test]
fn a_collision_keeps_the_base_part_and_is_reported() {
    let mut draft = draft();
    draft.modules.insert(module_id("mapper"), cache());
    draft.evidence.insert(
        CitationId("module:mapper".into()),
        draft.evidence[&CitationId(NOTE.into())].clone(),
    );
    draft.layers = Layers::new(vec![LayerName("Map".into())]).unwrap();
    let result = combined(&draft);
    assert_eq!(
        result.problems,
        [
            DraftProblem::LayerCollision {
                layer: LayerName("Map".into())
            },
            DraftProblem::Collision {
                part: MapPart::Module(module_id("mapper"))
            },
            DraftProblem::EvidenceCollision {
                citation: CitationId("module:mapper".into())
            },
        ]
    );
    assert_eq!(
        result.map.modules[&module_id("mapper")],
        sharsheret().modules[&module_id("mapper")]
    );
}

#[test]
fn an_update_fills_the_fields_it_leaves_out_from_the_base() {
    let mut draft = draft();
    draft.update.modules.insert(
        module_id("map-web"),
        update(ModuleChanges {
            kind: Some(ModuleKind::Logic),
            ..ModuleChanges::default()
        }),
    );
    let result = combined(&draft);
    assert_eq!(result.problems, []);
    let web = &result.map.modules[&module_id("map-web")];
    // The description stays the one found in code.
    assert_eq!(
        web.description,
        sharsheret().modules[&module_id("map-web")].description
    );
    let Status::ProposedUpdate {
        proposed:
            Review::Judged {
                judgment,
                reason,
                primary,
                ..
            },
    } = &web.status
    else {
        panic!("not an update: {:?}", web.status)
    };
    let Review::Judged { judgment: base, .. } = &web.description else {
        panic!("the example judges map-web")
    };
    assert_eq!(judgment.kind, ModuleKind::Logic);
    assert_eq!(
        (&judgment.concern, &judgment.layer),
        (&base.concern, &base.layer)
    );
    assert_eq!(
        (reason.as_str(), primary.0.as_str()),
        ("Rendering becomes pure.", NOTE)
    );
}

#[test]
fn a_connection_update_keeps_other_roles_it_leaves_out() {
    let mut base = sharsheret();
    let id = dependency_id("mapper", "map-web");
    if let Review::Judged { judgment, .. } =
        &mut base.dependencies.get_mut(&id).unwrap().description
    {
        judgment.other_roles = vec![UseRole::Incidental];
    }
    let mut draft = draft();
    draft.update.dependencies.insert(
        id.clone(),
        update(ConnectionChanges {
            role: Some(UseRole::Wires),
            other_roles: None,
        }),
    );
    let result = combine(&draft, Some(&base)).unwrap();
    assert_eq!(result.problems, []);
    let Status::ProposedUpdate {
        proposed: Review::Judged { judgment, .. },
    } = &result.map.dependencies[&id].status
    else {
        panic!("not an update")
    };
    assert_eq!(
        (judgment.role.clone(), judgment.other_roles.clone()),
        (UseRole::Wires, vec![UseRole::Incidental])
    );
}

#[test]
fn an_update_of_an_unreviewed_part_must_give_every_field() {
    let mut base = sharsheret();
    base.modules
        .get_mut(&module_id("map-web"))
        .unwrap()
        .description = Review::Unreviewed;
    let mut draft = draft();
    draft.update.modules.insert(
        module_id("map-web"),
        update(ModuleChanges {
            kind: Some(ModuleKind::Logic),
            ..ModuleChanges::default()
        }),
    );
    let result = combine(&draft, Some(&base)).unwrap();
    assert_eq!(
        result.problems,
        [DraftProblem::IncompleteUpdate {
            part: MapPart::Module(module_id("map-web")),
            missing: vec!["concern"],
        }]
    );
    assert_eq!(
        result.map.modules[&module_id("map-web")].status,
        Status::Observed
    );
}

#[test]
fn removing_a_module_removes_its_dependencies_and_routes() {
    let mut draft = draft();
    draft.remove.modules.insert(module_id("map-persist"));
    let result = combined(&draft);
    assert_eq!(result.problems, []);
    let map = &result.map;
    assert!(map.modules[&module_id("map-persist")].status.is_removal());
    let removed: Vec<&DependencyId> = map
        .dependencies
        .iter()
        .filter(|(_, d)| d.status.is_removal())
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        removed,
        [
            &dependency_id("map-persist", "project-map"),
            &dependency_id("mapper", "map-persist"),
        ]
    );
    let routes: Vec<&str> = map
        .routes
        .iter()
        .filter(|(_, r)| r.status.is_removal())
        .map(|(id, _)| id.0.as_str())
        .collect();
    assert_eq!(routes, ["save-the-project-map"]);
}

#[test]
fn updates_and_removals_of_missing_parts_are_reported() {
    let mut draft = draft();
    draft.update.routes.insert(
        RouteId("ghost".into()),
        update(ConnectionChanges {
            role: Some(UseRole::Wires),
            other_roles: None,
        }),
    );
    draft
        .remove
        .dependencies
        .insert(dependency_id("mapper", "ghost"));
    draft.remove.modules.insert(module_id("ghost"));
    assert_eq!(
        combined(&draft).problems,
        [
            DraftProblem::UpdateOfMissingPart {
                part: MapPart::Route(RouteId("ghost".into()))
            },
            DraftProblem::RemovalOfMissingPart {
                part: MapPart::Module(module_id("ghost"))
            },
            DraftProblem::RemovalOfMissingPart {
                part: MapPart::Dependency(dependency_id("mapper", "ghost"))
            },
        ]
    );
}

#[test]
fn a_part_updated_and_removed_keeps_the_removal_and_is_reported() {
    let mut draft = draft();
    let id = dependency_id("mapper", "map-persist");
    draft.update.dependencies.insert(
        id.clone(),
        update(ConnectionChanges {
            role: Some(UseRole::Wires),
            other_roles: None,
        }),
    );
    draft.remove.modules.insert(module_id("map-persist"));
    let result = combined(&draft);
    assert_eq!(
        result.problems,
        [DraftProblem::UpdatedAndRemoved {
            part: MapPart::Dependency(id.clone())
        }]
    );
    assert!(result.map.dependencies[&id].status.is_removal());
}

#[test]
fn a_change_to_a_part_already_proposed_in_the_base_is_refused() {
    let mut draft = draft();
    draft.update.routes.insert(
        RouteId(PROPOSED_ROUTE.into()),
        update(ConnectionChanges {
            role: Some(UseRole::Wires),
            other_roles: None,
        }),
    );
    assert_eq!(
        combined(&draft).problems,
        [DraftProblem::ChangeOfProposal {
            part: MapPart::Route(RouteId(PROPOSED_ROUTE.into()))
        }]
    );
}

#[test]
fn the_combined_map_is_checked_as_a_whole() {
    // A new dependency on a module the same draft removes.
    let mut draft = draft();
    draft.modules.insert(module_id("map-cache"), cache());
    draft.dependencies.insert(
        dependency_id("map-cache", "map-persist"),
        uses(UseRole::DirectUse),
    );
    draft.remove.modules.insert(module_id("map-persist"));
    assert_eq!(
        combined(&draft).problems,
        [DraftProblem::Map(MapProblem::ProposedRestsOnRemoval {
            part: MapPart::Dependency(dependency_id("map-cache", "map-persist")),
            removed: MapPart::Module(module_id("map-persist")),
        })]
    );
}

#[test]
fn a_draft_standing_alone_is_a_map_of_its_proposals() {
    let mut draft = draft();
    draft.base = DraftBase::None {
        project: "sketch".into(),
    };
    draft.layers = Layers::new(vec![LayerName("Map".into())]).unwrap();
    draft.modules.insert(module_id("map-cache"), cache());
    let result = combine(&draft, None).unwrap();
    assert_eq!(result.problems, []);
    assert_eq!(
        (result.map.project.as_str(), result.map.source.is_none()),
        ("sketch", true)
    );
    assert_eq!(result.map.modules.len(), 1);
    assert!(result.map.layers.contains(&LayerName("Map".into())));
}

#[test]
fn the_base_given_must_match_the_draft() {
    let mut alone = draft();
    alone.base = DraftBase::None {
        project: "sketch".into(),
    };
    assert_eq!(
        combine(&alone, Some(&sharsheret())).unwrap_err(),
        BaseMismatch::Unexpected
    );
    assert_eq!(
        combine(&draft(), None).unwrap_err(),
        BaseMismatch::Missing("map.toml".into())
    );
}
