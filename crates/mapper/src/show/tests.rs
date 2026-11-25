use super::*;
use project_map::{Draft, DraftBase, ProjectMap};
use std::path::Path;

#[derive(Default)]
struct Recorded {
    files: Vec<(PathBuf, String)>,
    served: Vec<SocketAddr>,
}

impl MapOutputs for Recorded {
    fn note(&mut self, _message: &str) {}

    fn write_file(&mut self, path: &Path, text: &str) -> std::io::Result<()> {
        self.files.push((path.into(), text.into()));
        Ok(())
    }

    fn serve_page(&mut self, address: SocketAddr, _page: &str) -> std::io::Error {
        self.served.push(address);
        std::io::Error::other("stopped")
    }
}

fn command() -> ShowCommand {
    ShowCommand {
        map: "saved.toml".into(),
        page: Some("map.html".into()),
        serve: None,
    }
}

fn saved(map: ProjectMap) -> String {
    map_persist::save(&MapDocument::Map(map))
}

#[test]
fn a_saved_map_is_shown_as_its_page() {
    let map = project_map::example::sharsheret();
    let mut outputs = Recorded::default();
    show(&command(), &saved(map.clone()), &mut outputs).unwrap();
    let [(path, page)] = outputs.files.as_slice() else {
        panic!("one page is written")
    };
    assert_eq!(path, Path::new("map.html"));
    assert_eq!(page, &map_web::page(&map));
}

#[test]
fn serving_reports_why_it_stopped() {
    let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let command = ShowCommand {
        page: None,
        serve: Some(address),
        ..command()
    };
    let mut outputs = Recorded::default();
    let error = show(
        &command,
        &saved(project_map::example::sharsheret()),
        &mut outputs,
    )
    .unwrap_err();
    assert!(matches!(error, ShowError::Present(MapError::Serve { .. })));
    assert_eq!(outputs.served, [address]);
}

#[test]
fn a_draft_is_refused_with_where_to_show_it() {
    let draft = Draft {
        base: DraftBase::None {
            project: "p".into(),
        },
        layers: project_map::Layers::empty(),
        modules: Default::default(),
        dependencies: Default::default(),
        routes: Default::default(),
        evidence: Default::default(),
        update: Default::default(),
        remove: Default::default(),
    };
    let text = map_persist::save(&MapDocument::Draft(draft));
    let error = show(&command(), &text, &mut Recorded::default()).unwrap_err();
    assert!(error.to_string().contains("srs draft"));
}

#[test]
fn text_that_does_not_load_names_the_file() {
    let error = show(&command(), "not a map", &mut Recorded::default()).unwrap_err();
    assert!(matches!(error, ShowError::Load { .. }));
    assert!(error.to_string().starts_with("saved.toml: "));
}

#[test]
fn without_a_page_or_address_nothing_is_done() {
    let command = ShowCommand {
        page: None,
        ..command()
    };
    let error = show(&command, "", &mut Recorded::default()).unwrap_err();
    assert!(matches!(error, ShowError::NothingToShow));
}

#[test]
fn a_map_whose_parts_do_not_fit_is_refused_before_anything_is_shown() {
    let mut map = project_map::example::sharsheret();
    map.modules.clear();
    let mut outputs = Recorded::default();
    let command = ShowCommand {
        serve: Some("127.0.0.1:8080".parse().unwrap()),
        ..command()
    };
    let error = show(&command, &saved(map.clone()), &mut outputs).unwrap_err();
    let ShowError::Problems { path, problems } = &error else {
        panic!("refused for its problems, not {error}")
    };
    assert_eq!(path, Path::new("saved.toml"));
    assert_eq!(problems, &map.problems());
    assert!(error.to_string().starts_with("saved.toml is not shown"));
    assert!(outputs.files.is_empty());
    assert!(outputs.served.is_empty());
}
