use super::*;
use project_graph::FactKind;
use project_graph_rust_records as r;
use std::cell::RefCell;
use std::path::PathBuf;

/// Stands in for Cargo: answers with the fixture's metadata and fixed records,
/// and remembers how it was asked to compile.
struct FakeCargo {
    compiled: RefCell<Vec<(Vec<String>, Vec<String>)>>,
}

impl Cargo for FakeCargo {
    fn metadata(&self, _: &Path) -> Result<String, GraphingError> {
        Ok(include_str!("metadata/fixture.json").to_string())
    }

    fn revision(&self, _: &Path) -> Result<String, GraphingError> {
        Ok("abc+local-changes".into())
    }

    fn compile(
        &self,
        _: &Path,
        args: &[String],
        members: &[String],
    ) -> Result<Vec<Vec<Record>>, GraphingError> {
        self.compiled
            .borrow_mut()
            .push((args.to_vec(), members.to_vec()));
        let def = |crate_name: &str, path: &str| r::Def {
            crate_name: crate_name.into(),
            crate_id: format!("{crate_name}-id"),
            path: path.into(),
            name: format!("{crate_name}{path}"),
        };
        Ok(vec![vec![
            Record::Unit(r::UnitRecord {
                package: "app".into(),
                crate_name: "app".into(),
                crate_id: "app-id".into(),
                crate_types: vec!["bin".into()],
                test: false,
                features: vec![],
                target_triple: "aarch64-unknown-linux-gnu".into(),
                externs: vec!["consumer".into()],
            }),
            Record::Item(r::ItemRecord {
                def: def("app", "::main"),
                kind: r::ItemKind::Function,
                exported: false,
                signature: Some("fn()".into()),
                span: None,
            }),
            Record::Call(r::CallRecord {
                owner: def("app", "::main"),
                callee: Some(def("clock", "::now")),
                dispatch: r::Dispatch::Direct,
                span: r::Span {
                    file: "app/src/main.rs".into(),
                    lo: 1,
                    hi: 2,
                },
            }),
        ]])
    }
}

fn grapher() -> RustGrapher<FakeCargo> {
    RustGrapher::new(FakeCargo {
        compiled: RefCell::new(Vec::new()),
    })
}

fn selection(target: &str) -> BuildSelection {
    BuildSelection {
        root: PathBuf::from("/work"),
        target: target.into(),
        configuration: "default features".into(),
    }
}

#[test]
fn the_graph_describes_the_selected_build() {
    let grapher = grapher();
    let graph = grapher.build_graph(&selection("app/bin:app")).unwrap();
    assert!(selection("app/bin:app").describes(&graph.build));
    assert_eq!(graph.build.project, "app");
    assert_eq!(graph.build.source_revision, "abc+local-changes");
    assert!(
        graph
            .facts
            .values()
            .any(|f| f.kind == FactKind::Call(project_graph::Dispatch::Direct))
    );
    let compiled = grapher.cargo.compiled.borrow();
    assert_eq!(compiled[0].0, ["-p", "app", "--bin", "app"]);
    assert_eq!(compiled[0].1, ["supplier", "consumer", "app"]);
}

#[test]
fn a_package_outside_the_workspace_is_refused_before_compiling() {
    let grapher = grapher();
    let error = grapher.build_graph(&selection("clock")).unwrap_err();
    assert!(error.message.contains("not a package in this workspace"));
    assert!(grapher.cargo.compiled.borrow().is_empty());
}

#[test]
fn the_pinned_toolchain_is_the_repository_toolchain() {
    let file = include_str!("../../../rust-toolchain.toml");
    assert!(
        file.contains(&format!("channel = \"{PINNED_TOOLCHAIN}\"")),
        "rust-toolchain.toml and PINNED_TOOLCHAIN must name the same version"
    );
}
