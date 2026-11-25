use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// Files in memory, and the notes the command printed.
#[derive(Default)]
struct Files {
    texts: RefCell<BTreeMap<PathBuf, String>>,
    written: BTreeMap<PathBuf, String>,
    notes: Vec<String>,
}

impl Files {
    fn with(contents: &[(&str, &str)]) -> Self {
        let files = Files::default();
        for (path, text) in files_list(contents) {
            files.texts.borrow_mut().insert(path, text);
        }
        files
    }

    fn set(&self, path: &str, text: &str) {
        self.texts
            .borrow_mut()
            .insert(PathBuf::from(path), text.into());
    }
}

fn files_list(files: &[(&str, &str)]) -> Vec<(PathBuf, String)> {
    files
        .iter()
        .map(|(path, text)| (PathBuf::from(path), text.to_string()))
        .collect()
}

impl DraftFiles for Files {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        self.texts
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"))
    }

    fn write(&mut self, path: &Path, text: &str) -> std::io::Result<()> {
        self.written.insert(path.into(), text.into());
        Ok(())
    }

    fn note(&mut self, message: &str) {
        self.notes.push(message.into());
    }
}

fn base() -> String {
    map_persist::save(&MapDocument::Map(project_map::example::sharsheret()))
}

/// A draft over `base.toml` beside it that adds one module.
const DRAFT: &str = r#"format = 1
kind = "draft"
base = "base.toml"

[modules.map-cache]
name = "map-cache"
review = { state = "unreviewed" }
"#;

fn files() -> Files {
    Files::with(&[("plans/draft.toml", DRAFT), ("plans/base.toml", &base())])
}

fn draft_path() -> PathBuf {
    PathBuf::from("plans/draft.toml")
}

#[test]
fn a_draft_is_laid_over_the_base_beside_it() {
    let built = build(&draft_path(), &files());
    let page = built.page.expect("the draft has no problems");
    assert!(page.contains("map-cache"));
    assert_eq!(
        built.sources,
        [draft_path(), PathBuf::from("plans/base.toml")]
    );
}

#[test]
fn check_reports_no_problems_or_each_problem_with_its_place() {
    let mut files = files();
    check(&draft_path(), &mut files).unwrap();
    assert_eq!(files.notes, ["plans/draft.toml: no problems"]);

    let mut files = files_with_draft(&DRAFT.replace("unreviewed", "unknown"));
    let error = check(&draft_path(), &mut files).unwrap_err();
    assert!(matches!(error, DraftError::Problems(1)));
    assert!(
        files.notes[0].starts_with("plans/draft.toml:7:"),
        "{}",
        files.notes[0]
    );
}

fn files_with_draft(draft: &str) -> Files {
    let files = files();
    files.set("plans/draft.toml", draft);
    files
}

#[test]
fn check_reports_what_does_not_fit_the_base() {
    let mut files = files_with_draft(&format!("{DRAFT}\n[remove]\nmodules = [\"ghost\"]\n"));
    assert!(check(&draft_path(), &mut files).is_err());
    assert_eq!(
        files.notes,
        ["plans/draft.toml: removes module ghost, which the base map does not have"]
    );
}

#[test]
fn a_missing_base_or_the_wrong_kind_of_file_is_a_problem() {
    let files = Files::with(&[("plans/draft.toml", DRAFT)]);
    assert_eq!(
        build(&draft_path(), &files).page,
        Err(vec!["plans/base.toml: no such file".to_string()])
    );
    let files = files_with_draft(&base());
    assert_eq!(
        build(&draft_path(), &files).page,
        Err(vec![
            "plans/draft.toml: is a saved map, not a draft".to_string()
        ])
    );
}

#[test]
fn render_writes_the_page_or_refuses_a_draft_with_problems() {
    let mut files = files();
    let out = PathBuf::from("draft.html");
    render(&draft_path(), &out, &mut files).unwrap();
    assert!(files.written[&out].contains("map-cache"));

    let mut files = files_with_draft("format = 2\nkind = \"draft\"\n");
    assert!(render(&draft_path(), &out, &mut files).is_err());
    assert!(files.written.is_empty());
}

/// Records what the preview was shown.
#[derive(Default)]
struct Sink(RefCell<Vec<Result<bool, Vec<String>>>>);

impl PreviewSink for Sink {
    fn show_page(&self, page: String) {
        self.0.borrow_mut().push(Ok(page.contains("map-cache")));
    }

    fn show_problems(&self, problems: Vec<String>) {
        self.0.borrow_mut().push(Err(problems));
    }
}

/// Makes each edit in turn when waited on, then stops the preview.
struct Edits<'a> {
    files: &'a Files,
    edits: Vec<String>,
    watched: Vec<Vec<PathBuf>>,
}

impl WatchFiles for Edits<'_> {
    fn wait(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        self.watched.push(paths.to_vec());
        if self.edits.is_empty() {
            return Err("stopped".into());
        }
        self.files.set("plans/draft.toml", &self.edits.remove(0));
        Ok(())
    }
}

#[test]
fn the_preview_shows_each_build_and_watches_the_draft_and_its_base() {
    let files = files();
    let sink = Sink::default();
    let mut edits = Edits {
        files: &files,
        edits: vec![
            DRAFT.replace("unreviewed", "unknown"),
            DRAFT.replace("map-cache", "map-store"),
        ],
        watched: vec![],
    };
    let error = preview(&draft_path(), &files, &sink, &mut edits);
    assert_eq!(error.to_string(), "watching the draft failed: stopped");
    let shown = sink.0.into_inner();
    assert_eq!(shown.len(), 3);
    assert_eq!(shown[0], Ok(true));
    assert!(matches!(&shown[1], Err(problems) if problems.len() == 1));
    assert_eq!(shown[2], Ok(false));
    let both = vec![draft_path(), PathBuf::from("plans/base.toml")];
    // A draft that fails to load names no base yet, so only it is watched.
    assert_eq!(edits.watched, [both.clone(), vec![draft_path()], both]);
}

#[test]
fn every_draft_problem_reads_as_a_sentence() {
    let module = MapPart::Module(project_map::ModuleId("cache".into()));
    let text = describe(&DraftProblem::IncompleteUpdate {
        part: module,
        missing: vec!["concern", "kind"],
    });
    assert_eq!(
        text,
        "updates module cache, which the base map has not judged, without concern or kind"
    );
}
