//! Presents a `ProjectMap` in the browser.
//!
//! [`page`] turns a map into one self-contained HTML document: the viewer's
//! markup, styles and script, with the map embedded as data. The page needs
//! no server and makes no network requests. Writing or serving it is up to the
//! caller.
//!
//! The page draws modules in their layers and dependencies as arrows colored
//! by role. Selecting a module shows its concern, surface, review, routes and
//! arrows. Each arrow it depends on or is used by shows how many uses it
//! summarizes and opens to the one-hop routes behind them. Its routes section
//! lists only routes of more than one hop; routes of one hop or none along
//! no arrow shown are listed as other uses. Each route lists its hops with
//! their evidence, whether each hop is established or only possible, and
//! where an incomplete trace stopped.
//! Unreviewed and deferred subjects stay on the map, marked as such.

mod view;

use project_map::ProjectMap;

const TEMPLATE: &str = include_str!("viewer/page.html");
const SCRIPT: &str = include_str!("viewer/viewer.js");
const DATA_SLOT: &str = "__MAP_DATA__";
const SCRIPT_SLOT: &str = "__VIEWER_SCRIPT__";

/// The HTML page presenting `map`.
pub fn page(map: &ProjectMap) -> String {
    // The view holds only strings, numbers, booleans and string-keyed maps,
    // which serde_json always serializes.
    let data = serde_json::to_string(&view::view(map)).expect("the map view serializes to JSON");
    // The script slot is filled first: the map data may contain either slot's text,
    // while the template's data slot precedes the script slot.
    TEMPLATE
        .replacen(SCRIPT_SLOT, SCRIPT, 1)
        .replacen(DATA_SLOT, &script_safe(&data), 1)
}

/// Keeps JSON inside a script element from closing it or reading as markup.
/// The escapes are valid inside JSON strings, where these characters can appear.
fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

#[cfg(test)]
mod tests;
