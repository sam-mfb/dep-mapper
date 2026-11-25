use super::*;
use std::fs;

/// A fresh directory for one test, under the system's temporary directory.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("map-preview-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_changed_file_is_reported_and_an_unchanged_one_is_not() {
    let dir = folder("changed");
    let (draft, base) = (dir.join("draft.toml"), dir.join("base.toml"));
    fs::write(&draft, "one").unwrap();
    fs::write(&base, "base").unwrap();
    let mut watcher =
        PollingWatcher::new(vec![draft.clone(), base], Duration::from_millis(5)).unwrap();
    let writer = std::thread::spawn({
        let draft = draft.clone();
        move || {
            std::thread::sleep(Duration::from_millis(30));
            fs::write(&draft, "two, longer").unwrap();
        }
    });
    assert_eq!(watcher.wait_for_change().unwrap(), [draft]);
    writer.join().unwrap();
}

#[test]
fn a_file_appearing_or_going_counts_as_a_change() {
    let dir = folder("appearing");
    let later = dir.join("later.toml");
    let mut watcher = PollingWatcher::new(vec![later.clone()], Duration::from_millis(5)).unwrap();
    fs::write(&later, "now").unwrap();
    assert_eq!(
        watcher.wait_for_change().unwrap(),
        std::slice::from_ref(&later)
    );
    fs::remove_file(&later).unwrap();
    assert_eq!(watcher.wait_for_change().unwrap(), [later]);
}
