use super::*;
use project_map::*;
use std::collections::{BTreeMap, BTreeSet};

fn round_trip(document: MapDocument) {
    let text = save(&document);
    let loaded = load(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    assert_eq!(loaded, document);
    assert_eq!(save(&loaded), text, "saving again gives the same text");
}

fn position(line: usize, column: usize) -> Option<Position> {
    Some(Position { line, column })
}

/// Loads `text`, which must fail, and returns the error.
fn refused(text: &str) -> LoadError {
    match load(text) {
        Ok(document) => panic!("loaded {document:?}"),
        Err(error) => error,
    }
}

fn proposed_route(origin: &str, receiver: &str, note: &str) -> MapRoute {
    MapRoute {
        status: Status::ProposedAddition,
        use_sites: Vec::new(),
        origin: RoutePoint::Project(ProjectPoint {
            module: ModuleId(origin.into()),
            item: Some("Thing".into()),
        }),
        hops: vec![
            MapHop {
                to: HopTarget::Project {
                    point: ProjectPoint {
                        module: ModuleId(receiver.into()),
                        item: None,
                    },
                    link: HopLink::ReceiverUsesSender,
                },
                evidence: vec![CitationId(note.into())],
                support: HopSupport::Proposed,
                role: Some(UseRole::Other("logs through".into())),
            },
            MapHop {
                to: HopTarget::Outside(OutsidePoint::Unresolved("plugin".into())),
                evidence: vec![CitationId(note.into())],
                support: HopSupport::Possible {
                    condition: Some("with plugins".into()),
                },
                role: None,
            },
        ],
        end: MapRouteEnd::Stopped {
            at: vec![CitationId(note.into())],
            reason: "Unknown plugin.".into(),
        },
        description: Review::Deferred {
            reason: "Not decided.".into(),
        },
    }
}

fn draft(base: DraftBase, update: Updates, remove: Removals) -> Draft {
    let note = CitationId("note".into());
    Draft {
        base,
        layers: Layers::new(vec![LayerName("Planned".into())]).unwrap(),
        modules: BTreeMap::from([(
            ModuleId("cache".into()),
            MapModule {
                status: Status::ProposedAddition,
                name: "cache".into(),
                surface: vec![SurfaceItem {
                    name: "Cache".into(),
                    kind: ItemKind::Type,
                    signature: None,
                    declaration: None,
                }],
                description: Review::Judged {
                    judgment: ModuleDescription {
                        concern: "Keeps recent maps.".into(),
                        kind: ModuleKind::Effect,
                        layer: Some(LayerName("Planned".into())),
                    },
                    reason: "Loading is slow.".into(),
                    primary: note.clone(),
                    supporting: vec![],
                },
            },
        )]),
        dependencies: BTreeMap::from([(
            DependencyId {
                from: ModuleId("cache".into()),
                to: ModuleId("map-persist".into()),
            },
            MapDependency {
                status: Status::ProposedAddition,
                evidence: vec![note.clone()],
                description: Review::Judged {
                    judgment: RouteDescription {
                        role: UseRole::DirectUse,
                        other_roles: vec![UseRole::Incidental],
                    },
                    reason: "Reads through it.".into(),
                    primary: note.clone(),
                    supporting: vec![],
                },
            },
        )]),
        routes: BTreeMap::from([(
            RouteId("cached".into()),
            proposed_route("map-persist", "cache", "note"),
        )]),
        evidence: BTreeMap::from([(
            note,
            MapEvidence {
                kind: EvidenceKind::Proposal,
                build_unit: None,
                summary: "Sam wants faster previews.".into(),
                location: None,
            },
        )]),
        update,
        remove,
    }
}

#[test]
fn a_saved_map_survives_saving_and_loading() {
    round_trip(MapDocument::Map(example::sharsheret()));
}

#[test]
fn a_map_without_a_source_and_with_located_evidence_survives_saving_and_loading() {
    let mut map = example::sharsheret();
    map.source = None;
    let gap = &mut map.coverage_gaps[0];
    gap.build_unit = Some(UnitId("map-web/lib".into()));
    gap.location = Some(Location {
        file: SourcePath("crates/map-web/src/lib.rs".into()),
        bytes: 3..9,
    });
    round_trip(MapDocument::Map(map));
}

#[test]
fn a_map_with_proposed_updates_survives_saving_and_loading() {
    let mut map = example::sharsheret();
    let module = map.modules.get_mut(&ModuleId("map-web".into())).unwrap();
    let mut proposed = module.description.clone();
    if let Review::Judged { judgment, .. } = &mut proposed {
        judgment.kind = ModuleKind::Logic;
    }
    module.status = Status::ProposedUpdate { proposed };
    let dependency = map.dependencies.values_mut().next().unwrap();
    dependency.status = Status::ProposedUpdate {
        proposed: Review::Deferred {
            reason: "Reconsidering.".into(),
        },
    };
    let route = map.routes.values_mut().next().unwrap();
    route.status = Status::ProposedRemoval;
    round_trip(MapDocument::Map(map));
}

#[test]
fn a_draft_over_a_base_survives_saving_and_loading() {
    let remove = Removals {
        modules: BTreeSet::from([ModuleId("map-web".into())]),
        dependencies: BTreeSet::from([DependencyId {
            from: ModuleId("mapper".into()),
            to: ModuleId("map-persist".into()),
        }]),
        routes: BTreeSet::from([RouteId("save-the-project-map".into())]),
    };
    let update = Updates {
        modules: BTreeMap::from([(
            ModuleId("mapper".into()),
            Update {
                changes: ModuleChanges {
                    concern: None,
                    kind: Some(ModuleKind::Logic),
                    layer: Some(LayerName("Planned".into())),
                },
                reason: "It only computes.".into(),
                primary: CitationId("note".into()),
                supporting: vec![],
            },
        )]),
        dependencies: BTreeMap::from([(
            DependencyId {
                from: ModuleId("mapper".into()),
                to: ModuleId("map-web".into()),
            },
            Update {
                changes: ConnectionChanges {
                    role: Some(UseRole::Wires),
                    other_roles: None,
                },
                reason: "It builds the page.".into(),
                primary: CitationId("note".into()),
                supporting: vec![CitationId("note".into())],
            },
        )]),
        routes: BTreeMap::from([(
            RouteId("present-the-project-map".into()),
            Update {
                changes: ConnectionChanges {
                    role: None,
                    other_roles: Some(vec![UseRole::Other("caches".into())]),
                },
                reason: "Cached now.".into(),
                primary: CitationId("note".into()),
                supporting: vec![],
            },
        )]),
    };
    round_trip(MapDocument::Draft(draft(
        DraftBase::Saved("maps/sharsheret.toml".into()),
        update,
        remove,
    )));
}

#[test]
fn a_draft_standing_alone_survives_saving_and_loading() {
    round_trip(MapDocument::Draft(draft(
        DraftBase::None {
            project: "sketch".into(),
        },
        Updates::default(),
        Removals::default(),
    )));
}

#[test]
fn a_hand_written_draft_leaves_out_statuses_and_empty_sections() {
    let text = r#"
format = 1
kind = "draft"
project = "sketch"

[modules.cache]
name = "cache"
review = { state = "unreviewed" }
"#;
    let Ok(MapDocument::Draft(draft)) = load(text) else {
        panic!("not a draft")
    };
    assert_eq!(
        draft.base,
        DraftBase::None {
            project: "sketch".into()
        }
    );
    assert!(draft.layers.is_empty());
    assert_eq!(
        draft.modules[&ModuleId("cache".into())].status,
        Status::ProposedAddition
    );
    assert!(draft.dependencies.is_empty() && draft.routes.is_empty());
    assert_eq!(draft.remove, Removals::default());
}

#[test]
fn an_unknown_version_is_refused_at_its_line() {
    let error = refused("kind = \"map\"\nformat = 3\n");
    assert_eq!(error.position, position(2, 10));
    assert!(error.message.contains("version 3"), "{}", error.message);
}

#[test]
fn an_unknown_kind_is_refused() {
    let error = refused("format = 1\nkind = \"sketch\"\n");
    assert_eq!(error.position, position(2, 8));
}

#[test]
fn a_missing_required_field_is_refused_with_a_position() {
    let error = refused(
        "format = 1\nkind = \"draft\"\nproject = \"p\"\n\n[modules.cache]\nreview = { state = \"unreviewed\" }\n",
    );
    assert!(error.message.contains("name"), "{}", error.message);
    assert_eq!(error.position.map(|p| p.line), Some(5));
}

#[test]
fn a_misspelled_field_is_refused_at_its_line() {
    let error = refused("format = 1\nkind = \"draft\"\nproject = \"p\"\nlayer = [\"x\"]\n");
    assert!(error.message.contains("layer"), "{}", error.message);
    assert_eq!(error.position, position(4, 1));
}

#[test]
fn a_saved_map_needs_every_status() {
    let mut text = save(&MapDocument::Map(example::sharsheret()));
    let at = text.find("status = \"observed\"\n").unwrap();
    text.replace_range(at..at + "status = \"observed\"\n".len(), "");
    let error = refused(&text);
    assert_eq!(error.message, "`status` is required in a saved map");
    assert!(error.position.is_some());
}

#[test]
fn a_draft_names_a_base_or_a_project_but_not_both() {
    let error = refused("format = 1\nkind = \"draft\"\nbase = \"m.toml\"\nproject = \"p\"\n");
    assert_eq!(error.position, position(4, 11));
    let error = refused("format = 1\nkind = \"draft\"\n");
    assert_eq!(error.position, None);
}

#[test]
fn a_draft_standing_alone_removes_nothing() {
    let error =
        refused("format = 1\nkind = \"draft\"\nproject = \"p\"\n\n[remove]\nmodules = [\"a\"]\n");
    assert_eq!(
        error.message,
        "a draft without a base map has nothing to remove"
    );
    let empty = "format = 1\nkind = \"draft\"\nproject = \"p\"\n\n[remove]\nmodules = []\n";
    assert!(load(empty).is_ok());
}

#[test]
fn a_part_removed_twice_is_refused_at_the_second() {
    let error = refused(
        "format = 1\nkind = \"draft\"\nbase = \"m\"\n\n[remove]\nroutes = [\"r\",\n  \"r\"]\n",
    );
    assert_eq!(error.position, position(7, 3));
}

const ROUTE: &str = r#"format = 1
kind = "draft"
project = "p"

[routes.r]
origin = { module = "a" }
end = { state = "reached" }
review = { state = "judged", role = "wires", reason = "Why.", primary = "n" }

[[routes.r.hops]]
to = { module = "b", link = "receiver-uses-sender" }
evidence = "n"
support = "proposed"
"#;

#[test]
fn a_route_written_by_hand_loads() {
    let Ok(MapDocument::Draft(draft)) = load(ROUTE) else {
        panic!("not a draft")
    };
    let route = &draft.routes[&RouteId("r".into())];
    assert_eq!(route.hops[0].support, HopSupport::Proposed);
    assert!(matches!(
        &route.description,
        Review::Judged { judgment, supporting, .. }
            if judgment.role == UseRole::Wires && supporting.is_empty()
    ));
}

/// `ROUTE` in format 2, using a site twice and stopping at two places.
const ROUTE_2: &str = r#"format = 2
kind = "draft"
project = "p"

[routes.r]
use_sites = ["s1", "s2"]
origin = { module = "a" }
end = { state = "stopped", at = ["s1", "s2"], reason = "Untraced." }
review = { state = "judged", role = "wires", reason = "Why.", primary = "n" }

[[routes.r.hops]]
to = { module = "b", link = "receiver-uses-sender" }
evidence = ["n", "m"]
support = "proposed"
"#;

#[test]
fn format_2_lists_every_site_and_fact() {
    let Ok(MapDocument::Draft(draft)) = load(ROUTE_2) else {
        panic!("not a draft")
    };
    let route = &draft.routes[&RouteId("r".into())];
    let ids = |names: &[&str]| -> Vec<CitationId> {
        names.iter().map(|n| CitationId(n.to_string())).collect()
    };
    assert_eq!(route.use_sites, ids(&["s1", "s2"]));
    assert_eq!(route.hops[0].evidence, ids(&["n", "m"]));
    assert_eq!(
        route.end,
        MapRouteEnd::Stopped {
            at: ids(&["s1", "s2"]),
            reason: "Untraced.".into(),
        }
    );
    round_trip(MapDocument::Draft(draft));
}

#[test]
fn format_1_still_loads_each_citation_as_a_list_of_one() {
    let text = ROUTE.replace(
        "origin = { module = \"a\" }",
        "use_site = \"s\"\norigin = { module = \"a\" }",
    );
    let Ok(MapDocument::Draft(draft)) = load(&text) else {
        panic!("not a draft")
    };
    let route = &draft.routes[&RouteId("r".into())];
    assert_eq!(route.use_sites, [CitationId("s".into())]);
    assert_eq!(route.hops[0].evidence, [CitationId("n".into())]);
    let saved = save(&MapDocument::Draft(draft.clone()));
    assert!(saved.starts_with("format = 2"), "{saved}");
    assert!(saved.contains("use_sites = [\"s\"]"), "{saved}");
    assert!(saved.contains("evidence = [\"n\"]"), "{saved}");
    assert_eq!(load(&saved).unwrap(), MapDocument::Draft(draft));
}

#[test]
fn citations_must_take_the_form_of_their_format() {
    let error = refused(&ROUTE_2.replace("use_sites = [\"s1\", \"s2\"]", "use_site = \"s1\""));
    assert!(
        error.message.contains("format 2 has `use_sites`"),
        "{}",
        error.message
    );
    assert_eq!(error.position, position(6, 12));
    let error = refused(&ROUTE_2.replace("evidence = [\"n\", \"m\"]", "evidence = \"n\""));
    assert_eq!(
        error.message,
        "in format 2, a hop's `evidence` is a list of at least one citation ID"
    );
    assert_eq!(error.position, position(13, 12));
    let error = refused(&ROUTE_2.replace("evidence = [\"n\", \"m\"]", "evidence = []"));
    assert!(error.message.contains("at least one"), "{}", error.message);
    let error = refused(&ROUTE_2.replace("at = [\"s1\", \"s2\"]", "at = []"));
    assert_eq!(
        error.message,
        "in format 2, a stopped route's `at` is a list of at least one citation ID"
    );
    let error = refused(&ROUTE.replace("evidence = \"n\"", "evidence = [\"n\"]"));
    assert_eq!(
        error.message,
        "in format 1, a hop's `evidence` is one citation ID"
    );
    let error = refused(&ROUTE.replace(
        "origin = { module = \"a\" }",
        "use_sites = [\"s\"]\norigin = { module = \"a\" }",
    ));
    assert!(
        error.message.contains("format 1 has `use_site`"),
        "{}",
        error.message
    );
}

#[test]
fn an_unknown_role_is_refused_at_its_place() {
    let error = refused(&ROUTE.replace("role = \"wires\"", "role = \"wire\""));
    assert_eq!(error.position, position(8, 37));
    assert!(
        error.message.contains("unknown role \"wire\""),
        "{}",
        error.message
    );
}

#[test]
fn a_review_field_that_does_not_fit_its_state_is_refused() {
    let error = refused(&ROUTE.replace("state = \"judged\"", "state = \"deferred\""));
    assert_eq!(error.message, "a deferred review has no `role`");
    assert_eq!(error.position, position(8, 10));
    let error = refused(&ROUTE.replace("role = \"wires\"", "kind = \"logic\""));
    assert_eq!(error.message, "a judged review has no `kind`");
    let error = refused(&ROUTE.replace(", primary = \"n\"", ""));
    assert_eq!(error.message, "a judged review needs `primary`");
}

#[test]
fn a_point_must_be_one_kind_of_place() {
    let error = refused(&ROUTE.replace(
        "origin = { module = \"a\" }",
        "origin = { module = \"a\", external = \"tokio\" }",
    ));
    assert_eq!(error.position, position(6, 10));
    let error = refused(&ROUTE.replace(", link = \"receiver-uses-sender\"", ""));
    assert_eq!(error.message, "a hop into a project module needs a `link`");
    let error = refused(&ROUTE.replace(
        "origin = { module = \"a\" }",
        "origin = { module = \"a\", link = \"same-module\" }",
    ));
    assert_eq!(error.message, "a route's origin has no `link`");
}

#[test]
fn only_a_possible_hop_has_a_condition() {
    let error = refused(&ROUTE.replace(
        "support = \"proposed\"",
        "support = \"proposed\"\ncondition = \"later\"",
    ));
    assert_eq!(error.message, "only a possible hop has a `condition`");
    assert_eq!(error.position, position(13, 11));
}

#[test]
fn a_dependency_listed_twice_is_refused() {
    let entry = "[[dependencies]]\nfrom = \"a\"\nto = \"b\"\nreview = { state = \"unreviewed\" }\n";
    let text = format!("format = 1\nkind = \"draft\"\nproject = \"p\"\n\n{entry}\n{entry}");
    let error = refused(&text);
    assert_eq!(error.message, "the dependency of a on b is listed twice");
    assert_eq!(error.position.map(|p| p.line), Some(10));
}

#[test]
fn a_location_ending_before_it_starts_is_refused() {
    let text = "format = 1\nkind = \"draft\"\nproject = \"p\"\n\n[evidence.n]\nkind = \"source\"\nsummary = \"s\"\nlocation = { file = \"f\", start = 9, end = 3 }\n";
    let error = refused(text);
    assert_eq!(error.message, "`start` (9) is after `end` (3)");
    assert_eq!(error.position, position(8, 12));
}

#[test]
fn an_error_reads_as_line_column_and_message() {
    let error = LoadError {
        position: position(3, 7),
        message: "bad".into(),
    };
    assert_eq!(error.to_string(), "3:7: bad");
}

#[test]
fn the_examples_in_the_format_document_load_as_one_draft() {
    let document = include_str!("../FORMAT.md");
    let blocks: Vec<&str> = document
        .split("```toml\n")
        .skip(1)
        .map(|block| block.split("```").next().unwrap())
        .collect();
    assert_eq!(blocks.len(), 5);
    let text = format!(
        "format = 2\nkind = \"draft\"\nbase = \"m.toml\"\n\n{}",
        blocks.join("\n")
    );
    let Ok(MapDocument::Draft(draft)) = load(&text) else {
        panic!("{:?}", load(&text))
    };
    assert_eq!(
        (
            draft.modules.len(),
            draft.dependencies.len(),
            draft.routes.len()
        ),
        (1, 1, 1)
    );
    assert!(
        draft
            .modules
            .values()
            .all(|m| m.status == Status::ProposedAddition)
    );
    assert_eq!(draft.update.modules.len(), 1);
}

const UPDATE: &str = r#"format = 1
kind = "draft"
base = "map.toml"

[update.modules.mapper]
kind = "logic"
reason = "It only computes."
primary = "note"

[update.routes.r]
role = "incidental"
reason = "Rarely used."
primary = "note"
"#;

#[test]
fn an_update_written_by_hand_gives_only_what_changes() {
    let Ok(MapDocument::Draft(draft)) = load(UPDATE) else {
        panic!("{:?}", load(UPDATE))
    };
    let module = &draft.update.modules[&ModuleId("mapper".into())];
    assert_eq!(
        module.changes,
        ModuleChanges {
            concern: None,
            kind: Some(ModuleKind::Logic),
            layer: None,
        }
    );
    assert!(module.supporting.is_empty());
    let route = &draft.update.routes[&RouteId("r".into())];
    assert_eq!(route.changes.role, Some(UseRole::Incidental));
    assert_eq!(route.changes.other_roles, None);
}

#[test]
fn an_update_must_change_a_field_that_fits_its_part() {
    let error = refused(&UPDATE.replace("kind = \"logic\"\n", ""));
    assert_eq!(error.message, "an update changes at least one field");
    assert_eq!(error.position.map(|p| p.line), Some(5));
    let error = refused(&UPDATE.replace("kind = \"logic\"", "role = \"wires\""));
    assert_eq!(
        error.message,
        "a module's update has no `role` or `other_roles`"
    );
    let error = refused(&UPDATE.replace("role = \"incidental\"", "concern = \"Other.\""));
    assert_eq!(
        error.message,
        "a route's update has no `concern`, `kind` or `layer`"
    );
}

#[test]
fn a_draft_standing_alone_updates_nothing() {
    let error = refused(&UPDATE.replace("base = \"map.toml\"", "project = \"p\""));
    assert_eq!(
        error.message,
        "a draft without a base map has nothing to update"
    );
}

#[test]
fn a_dependency_updated_twice_is_refused() {
    let entry = "[[update.dependencies]]\nfrom = \"a\"\nto = \"b\"\nrole = \"wires\"\nreason = \"r\"\nprimary = \"n\"\n";
    let error = refused(&format!(
        "format = 1\nkind = \"draft\"\nbase = \"m\"\n\n{entry}\n{entry}"
    ));
    assert_eq!(error.message, "the dependency of a on b is updated twice");
}

#[test]
fn a_drafts_own_parts_are_additions() {
    let text = "format = 1\nkind = \"draft\"\nproject = \"p\"\n\n[modules.a]\nstatus = \"observed\"\nname = \"a\"\nreview = { state = \"unreviewed\" }\n";
    let error = refused(text);
    assert!(
        error
            .message
            .starts_with("a draft's own parts are additions"),
        "{}",
        error.message
    );
    assert_eq!(error.position.map(|p| p.line), Some(5));
    assert!(load(&text.replace("observed", "proposed-addition")).is_ok());
}

#[test]
fn only_a_proposed_update_in_a_map_carries_a_proposed_review() {
    let module = "[modules.a]\nstatus = \"proposed-update\"\nname = \"a\"\nreview = { state = \"unreviewed\" }\n";
    let map = format!("format = 1\nkind = \"map\"\nproject = \"p\"\nlayers = []\n\n{module}");
    let error = refused(&map);
    assert_eq!(error.message, "a proposed update needs a `proposed` review");
    let with_review = format!("{map}proposed = {{ state = \"deferred\", reason = \"Later.\" }}\n");
    assert!(load(&with_review).is_ok(), "{:?}", load(&with_review));
    let error = refused(&with_review.replace("proposed-update", "observed"));
    assert_eq!(
        error.message,
        "only a proposed update has a `proposed` review"
    );
    assert_eq!(error.position, position(10, 12));
}

#[test]
fn a_role_the_reviewer_named_reads_back_exactly() {
    for name in ["", " padded ", "logs through", "other: nested"] {
        let mut map = example::sharsheret();
        let route = map.routes.values_mut().next().unwrap();
        route.description = Review::Judged {
            judgment: RouteDescription {
                role: UseRole::Other(name.into()),
                other_roles: vec![],
            },
            reason: "Named.".into(),
            primary: CitationId("module:mapper".into()),
            supporting: vec![],
        };
        round_trip(MapDocument::Map(map));
    }
    let Ok(MapDocument::Draft(draft)) =
        load(&ROUTE.replace("role = \"wires\"", "role = \"other:logs\""))
    else {
        panic!("not a draft")
    };
    assert!(matches!(
        &draft.routes[&RouteId("r".into())].description,
        Review::Judged { judgment, .. } if judgment.role == UseRole::Other("logs".into())
    ));
}
