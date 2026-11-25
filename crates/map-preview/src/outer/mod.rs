//! The page the browser opens. It holds the map page in a frame, lists the
//! problems the preview reports, and reloads the frame when the server sends
//! a new version. The frame keeps its address fragment across reloads, so
//! map-web keeps the reader's place.

pub const OUTER_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Map preview</title>
<style>
  :root { --bg: #f7f7f4; --fg: #1d1d21; --line: #deded8; --bad: #b42318; --bad-bg: #fff1ef; color-scheme: light; }
  @media (prefers-color-scheme: dark) {
    :root { --bg: #16171a; --fg: #e8e8ea; --line: #34353b; --bad: #f97066; --bad-bg: #3a1d1b; color-scheme: dark; }
  }
  html, body { margin: 0; height: 100%; background: var(--bg); color: var(--fg); font: 14px/1.45 system-ui, sans-serif; }
  body { display: flex; flex-direction: column; }
  #problems { border-bottom: 1px solid var(--line); background: var(--bad-bg); color: var(--bad); padding: 8px 16px; }
  #problems[hidden] { display: none; }
  #problems h2 { font-size: 13px; margin: 0 0 4px; }
  #problems ul { margin: 0; padding-left: 18px; font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 12.5px; }
  #waiting { padding: 16px; }
  #waiting[hidden], #map[hidden] { display: none; }
  #map { flex: 1; width: 100%; border: 0; }
</style>
</head>
<body>
<section id="problems" hidden aria-live="polite">
  <h2>The draft has problems. The map below is the last one that built.</h2>
  <ul id="problem-list"></ul>
</section>
<p id="waiting">Waiting for the first map…</p>
<iframe id="map" title="Map" hidden></iframe>
<script>
"use strict";
(function () {
  const frame = document.getElementById("map");
  const list = document.getElementById("problem-list");
  let shown = -1;
  // Reloads the frame at the same place in the map.
  function load(version) {
    let place = "";
    try { place = frame.contentWindow.location.hash; } catch (_) { /* not loaded yet */ }
    frame.src = "/map?v=" + version + place;
  }
  // `status.page` is the version of the last good page, or null before any.
  // The frame reloads whenever that differs from the page it shows, even
  // while there are problems: the browser may have missed the newer page
  // while disconnected. Problems alone leave the frame as it is.
  function show(status) {
    list.replaceChildren(...status.problems.map((problem) => {
      const item = document.createElement("li");
      item.textContent = problem;
      return item;
    }));
    const hasPage = status.page !== null;
    document.getElementById("problems").hidden = status.problems.length === 0;
    document.getElementById("waiting").hidden = hasPage;
    frame.hidden = !hasPage;
    if (hasPage && status.page !== shown) {
      shown = status.page;
      load(status.page);
    }
  }
  // The browser reconnects by itself if the connection drops.
  new EventSource("/events").onmessage = (event) => show(JSON.parse(event.data));
})();
</script>
</body>
</html>
"#;
