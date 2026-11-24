use super::*;
use project_map::example::dep_mapper;

/// The map data embedded in a page, parsed back from the page text.
fn embedded(page: &str) -> serde_json::Value {
    let start = page.find(r#"id="map-data">"#).expect("data element") + r#"id="map-data">"#.len();
    let end = start
        + page[start..]
            .find("</script>")
            .expect("data element closes");
    serde_json::from_str(&page[start..end]).expect("embedded data is JSON")
}

#[test]
fn the_page_embeds_the_map_and_the_viewer_once() {
    let page = page(&dep_mapper());
    assert_eq!(embedded(&page)["meta"]["project"], "dep-mapper");
    assert_eq!(page.matches(SCRIPT).count(), 1);
    assert!(!page.contains(DATA_SLOT) && !page.contains(SCRIPT_SLOT));
}

#[test]
fn markup_in_map_text_cannot_close_the_data_element() {
    let mut map = dep_mapper();
    map.project = "</script><script>alert(1)</script> & co".into();
    let page = page(&map);
    assert!(!page.contains("<script>alert(1)"));
    assert_eq!(embedded(&page)["meta"]["project"], map.project.as_str());
}

#[test]
fn slot_names_in_map_text_are_kept_as_data() {
    let mut map = dep_mapper();
    map.project = format!("{SCRIPT_SLOT} {DATA_SLOT}");
    let page = page(&map);
    assert_eq!(page.matches(SCRIPT).count(), 1);
    assert_eq!(embedded(&page)["meta"]["project"], map.project.as_str());
}

#[test]
fn the_viewer_script_cannot_end_its_own_element() {
    assert!(!SCRIPT.to_ascii_lowercase().contains("</script"));
}
