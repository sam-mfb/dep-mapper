//! Pin the vocabulary to source from the independently compiled proof fixture.
//! These are hand-recorded witnesses, so this test needs no compiler collector.
use project_graph::*;
use std::collections::BTreeMap;

const CONSOLE: &str = include_str!("../../../proof/compiler/sample/console/src/main.rs");

fn span(source: &str) -> SourceSpan {
    let start = CONSOLE.find(source).expect("proof fixture expression");
    SourceSpan {
        file: "console/src/main.rs".into(),
        bytes: start..start + source.len(),
    }
}

fn item(key: &str) -> Endpoint {
    Endpoint::Item(ItemId(key.into()))
}

#[test]
fn adjacent_calls_and_reexports_keep_their_own_targets_and_source() {
    let unit = UnitId("console/bin".into());
    let owner = item("console::Bridge::read");
    let call = |target: &str, text: &str| Evidence {
        unit: unit.clone(),
        kind: Fact::Call(Dispatch::Direct),
        subject: owner.clone(),
        object: item(target),
        site: Some(span(text)),
    };
    let audit = call("sensor::audit", "instrument::audit()");
    let measure = call("console::sample", "sample()\n    }");
    let export = Evidence {
        unit,
        subject: Endpoint::Module(ModuleId("console".into())),
        kind: Fact::Reexport {
            exported_name: "sample_value".into(),
        },
        object: item("sensor::measured"),
        site: Some(span("pub use instrument::measured as sample_value;")),
    };
    assert_ne!(audit.object, measure.object);
    assert_ne!(audit.site, measure.site);
    assert_eq!(export.object, item("sensor::measured"));
    let site = export.site.unwrap();
    assert_eq!(
        &CONSOLE[site.bytes],
        "pub use instrument::measured as sample_value;"
    );
}

#[test]
fn two_possible_providers_can_reach_one_use_and_a_trace_can_stop_before_reaching_it() {
    let used = EvidenceId("display:panel.active.read".into());
    let endpoint = item("port::display");
    let providers = [
        ("console::Bridge", "adapted"),
        ("sensor::Factory", "!adapted"),
    ];
    let routes: BTreeMap<_, _> = providers
        .into_iter()
        .map(|(provider, condition)| {
            let id = RouteId(format!("{provider} through active"));
            let route = Route {
                origin: item(provider),
                use_site: Some(used.clone()),
                hops: vec![Hop {
                    to: endpoint.clone(),
                    evidence: EvidenceId("console:active=choose(adapted)".into()),
                    support: Support::Possible {
                        condition: Some(condition.into()),
                    },
                }],
                end: RouteEnd::ReachedUse,
            };
            (id, route)
        })
        .collect();
    assert_eq!(routes.len(), 2);
    assert!(routes.values().all(|r| r.use_site.as_ref() == Some(&used)));
    let stopped = Route {
        origin: item("console::edit"),
        use_site: Some(EvidenceId("port::after_edit:value.read".into())),
        hops: vec![],
        end: RouteEnd::Stopped {
            at: EvidenceId("console:edit(&mut slot)".into()),
            reason: "mutable borrow on incoming path".into(),
        },
    };
    assert!(matches!(stopped.end, RouteEnd::Stopped { .. }));
    assert_eq!(&CONSOLE[span("edit(&mut slot)").bytes], "edit(&mut slot)");
}

#[test]
fn identifying_a_receiver_package_does_not_require_inventing_a_handoff() {
    let call = EvidenceId("agent.rs:105 CredentialSlot::save".into());
    let bound = Evidence {
        unit: UnitId("kkt-agent-commands/lib".into()),
        subject: Endpoint::Unresolved("receiver".into()),
        object: Endpoint::Module(ModuleId("kashkeshet".into())),
        kind: Fact::ReceiverBound {
            call: call.clone(),
            basis: ReceiverBasis::UniqueInClosedBuild,
        },
        site: Some(SourceSpan {
            file: "crates/kkt-agent-commands/src/agent.rs".into(),
            bytes: 3619..3656,
        }),
    };
    let partial = Route {
        use_site: Some(call.clone()),
        origin: bound.object.clone(),
        hops: vec![],
        end: RouteEnd::Stopped {
            at: call.clone(),
            reason: "No source-backed handoff recovered for this use.".into(),
        },
    };
    let Fact::ReceiverBound { call: site, .. } = bound.kind else {
        panic!("expected receiver bound")
    };
    assert_eq!(partial.use_site.as_ref(), Some(&site));
    assert!(partial.hops.is_empty());
}
