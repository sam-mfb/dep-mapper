use super::*;

#[test]
fn a_new_page_replaces_the_last_and_clears_the_problems() {
    let mut shown = Shown::new();
    assert_eq!((shown.version(), shown.page()), (0, None));
    shown.show_problems(vec!["bad".into()]);
    shown.show_page("one".into());
    assert_eq!(shown.version(), 2);
    assert_eq!(shown.page(), Some("one"));
    assert!(shown.problems().is_empty());
}

#[test]
fn problems_keep_the_last_good_page() {
    let mut shown = Shown::new();
    shown.show_page("one".into());
    shown.show_problems(vec!["3:7: unknown role".into()]);
    assert_eq!(shown.page(), Some("one"));
    assert_eq!(shown.problems(), ["3:7: unknown role"]);
    assert_eq!(shown.version(), 2);
}

#[test]
fn the_status_tells_a_browser_the_version_and_problems() {
    let mut shown = Shown::new();
    shown.show_problems(vec!["a \"quoted\" problem".into()]);
    let status: serde_json::Value = serde_json::from_str(&shown.status_json()).unwrap();
    assert_eq!(
        status,
        serde_json::json!({ "version": 1, "page": false, "problems": ["a \"quoted\" problem"] })
    );
}
