use super::*;
use std::fs;

/// A fresh Git repository with one committed file, in a temporary directory.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("srs-revision-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo(dir);
        repo.git(&["init", "-q"]);
        repo.write("a.ts", "export const a = 1;\n");
        repo.git(&["add", "."]);
        repo.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "one",
        ]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn write(&self, file: &str, contents: &str) {
        fs::write(self.0.join(file), contents).unwrap();
    }

    fn revision(&self) -> String {
        GitRevision { git: "git".into() }.revision(&self.0).unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_clean_tree_is_its_commit() {
    let repo = Repo::new("clean");
    let revision = repo.revision();
    assert_eq!(revision.len(), 40, "{revision}");
}

#[test]
fn different_local_changes_give_different_revisions() {
    let repo = Repo::new("tracked");
    let clean = repo.revision();
    repo.write("a.ts", "export const probeOne = 1;\n");
    let one = repo.revision();
    repo.write("a.ts", "export const probeTwo = 2;\n");
    let two = repo.revision();
    assert!(one.starts_with(&format!("{clean}-dirty-")), "{one}");
    assert_ne!(one, two);
    repo.write("a.ts", "export const probeOne = 1;\n");
    assert_eq!(repo.revision(), one);
}

#[test]
fn untracked_file_contents_count_as_changes() {
    let repo = Repo::new("untracked");
    repo.write("b.ts", "export const b = 1;\n");
    let one = repo.revision();
    repo.write("b.ts", "export const b = 2;\n");
    assert_ne!(repo.revision(), one);
}

#[test]
fn a_directory_outside_git_is_not_versioned() {
    let dir = std::env::temp_dir().join(format!("srs-revision-plain-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = GitRevision { git: "git".into() }.revision(&dir);
    fs::remove_dir_all(&dir).unwrap();
    assert!(
        matches!(result, Err(RevisionError::NotVersioned)),
        "{result:?}"
    );
}

#[test]
fn a_missing_git_program_is_a_failure() {
    let repo = Repo::new("no-git");
    let result = GitRevision {
        git: "/no/such/git".into(),
    }
    .revision(&repo.0);
    assert!(
        matches!(result, Err(RevisionError::Failed(_))),
        "{result:?}"
    );
}
