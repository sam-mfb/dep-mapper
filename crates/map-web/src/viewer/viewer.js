// map-web's viewer: the Kashkeshet map renderer adapted to draw a ProjectMap.
// map-web embeds the map's view as JSON in the element with id "map-data".
"use strict";

(function () {
  const graph = JSON.parse(document.getElementById("map-data").textContent);
  // The map names its layers; modules whose layer is not listed are in map-web's
  // Unassigned band, which the view adds to the list.
  const concerns = {
    layers: graph.layers.map((name) => ({ id: name, title: name })),
    kinds: [
      { id: "logic", title: "Logic", about: "Computes results using its inputs and supplied capabilities." },
      { id: "effect", title: "Effect", about: "Interacts with something outside the program, such as files, a terminal or a network service." },
      { id: "contract", title: "Contract", about: "Defines a shared public boundary." },
      { id: "composition", title: "Composition", about: "Supplies implementations and brings modules together." },
      { id: "unreviewed", title: "Unreviewed", about: "No reviewer has judged this module's concern, kind or layer." },
    ],
  };
  // Edge records point from the dependent module to its dependency.
  // An arrow whose first role supplies input points from the caller to the receiver.
  const ROLES = {
    implements: "implemented by",
    adapts: "adapted by",
    injects: "injected into",
    uses: "used by",
    supplies: "supplies input to",
    wires: "built by",
    incidental: "incidentally used by",
    other: "otherwise used by",
    unreviewed: "used by (unreviewed)",
  };
  const ROLE_ABOUT = {
    implements: "the module at the arrow's head implements a contract the module at its tail defines",
    adapts: "the module at the arrow's head delivers the work of the module at its tail through an adapter of its own",
    injects: "the work of the module at the arrow's tail reaches the module at its head as an implementation of a contract",
    supplies: "the module at the arrow's tail calls the module at its head with an input",
    uses: "the module at the arrow's head uses a type, value or operation from the module at its tail",
    wires: "the module at the arrow's head builds implementations or holders from the module at its tail, to hand on",
    incidental: "the module at the arrow's head uses the module at its tail, but not for its main work",
    other: "a use the reviewer named; the arrow's details give the name",
    unreviewed: "a dependency no reviewer has judged",
  };
  // A role's name on its own, for a route or one of its hops.
  const ROLE_NAMES = {
    implements: "implements", adapts: "adapted injection", injects: "full injection", uses: "direct use",
    supplies: "supplies input", wires: "wires", incidental: "incidental use", other: "other use", unreviewed: "unreviewed",
  };
  // The roles the map's arrows use, in the order ROLES lists them.
  const usedRoles = () => Object.keys(ROLES).filter((r) => graph.edges.some((e) => e.roles.includes(r)));
  const drawnEnds = (e) => e.roles[0] === "supplies" ? [e.from, e.to] : [e.to, e.from];
  // How far apart the parallel strokes of an arrow with several roles are.
  const STROKE_GAP = 3.2;
  // How many straight pieces an offset stroke is drawn with.
  const EDGE_STEPS = 32;
  const SVG = "http://www.w3.org/2000/svg";
  const BOX_W = 156;
  const BOX_H = 42;
  // Where a box's text starts; it ends as far from the right edge.
  const LABEL_X = 9;
  const GAP_X = 18;
  const PAD_X = 16;
  const PAD_Y = 14;

  const crates = new Map(graph.crates.map((c) => [c.name, c]));
  const state = {
    selected: null,
    // The crates whose checkbox is ticked, by name: they light up their
    // arrows, neighbours and delivery lines. `selected` is the crate the
    // panel shows; clicking a box sets it and highlights nothing. Both are
    // kept by name whatever the filters draw, and the map is drawn from them.
    checked: new Set(),
    hiddenPrograms: new Set(),
    hiddenKinds: new Set(),
    hiddenRoles: new Set(),
    hiddenLayers: new Set(),
    query: "",
  };

  const $ = (id) => document.getElementById(id);
  const el = (tag, attrs, ...children) => {
    const node = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (k === "class") node.className = v;
      else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
      else node.setAttribute(k, v);
    }
    for (const child of children.flat(Infinity)) {
      if (child == null || child === false) continue;
      node.append(child instanceof Node ? child : document.createTextNode(String(child)));
    }
    return node;
  };
  const svg = (tag, attrs) => {
    const node = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs || {})) node.setAttribute(k, v);
    return node;
  };
  // Text with `code` spans for backticked words, built without innerHTML.
  const prose = (text) => {
    const parts = String(text || "").split(/`([^`]+)`/);
    return parts.map((part, i) => (i % 2 ? el("code", {}, part) : part));
  };

  const concernOf = (name) => crates.get(name) || {};
  const kindSlug = (kind) => String(kind || "").replace(/ /g, "-");
  const kindAbout = (kind) => ((concerns.kinds || []).find((k) => k.id === kind) || {}).about || "";
  // Every crate keeps its place whatever is drawn (see layout), so hiding a
  // kind leaves empty slots.
  const crateKind = (name) => concernOf(name).kind || "";
  const kindShown = (name) => !state.hiddenKinds.has(crateKind(name));
  // A crate in a hidden layer keeps its place and is drawn faded, and
  // still opens its panel; no arrow or delivery line goes to or from it.
  const crateLayer = (name) => concernOf(name).layer || "";
  const layerShown = (name) => !state.hiddenLayers.has(crateLayer(name));
  const drawnCrate = (name) => kindShown(name);
  // A program hidden by the eye on its box is faded like a hidden layer's
  // crates, with no arrow or delivery line from it.
  const programShown = (name) => !state.hiddenPrograms.has(name);
  const visibleCrate = (name) => drawnCrate(name) && layerShown(name) && programShown(name);
  const shownRoles = (e) => e.roles.filter((r) => !state.hiddenRoles.has(r));
  const visibleEdge = (e) => visibleCrate(e.from) && visibleCrate(e.to) && shownRoles(e).length > 0;

  // ---- Layout: one band per layer, each band ordered to cut crossings ----
  // A band holds up to PER_LINE units on a line, and wraps onto more
  // lines of even length. A unit is one crate, or a group (the view's
  // `groups`): its head crate with its partners stacked directly beneath.

  // Five at most, so no band is wider than five boxes.
  const columns = () => window.matchMedia("(max-width: 600px)").matches ? 2 : 4;
  const TITLE_H = 24;
  const LINE_H = BOX_H + 30;
  // The space between the boxes of a group, where the arrow from a
  // partner to its head runs.
  const STACK_GAP = 20;
  // How far a group's outline stands off its boxes.
  const GROUP_PAD = 6;

  // The groups, by head, and the head each partner goes under. A group
  // whose crates are not all on the map is ignored.
  const groupsOf = new Map(Object.entries(graph.groups || {})
    .filter(([head, partners]) => crates.has(head) && partners.every((n) => crates.has(n))));
  const headOf = new Map([...groupsOf].flatMap(([head, partners]) => partners.map((n) => [n, head])));
  const groupOf = (name) => {
    const head = headOf.get(name) || name;
    return [head, ...(groupsOf.get(head) || [])];
  };

  // Every box's place is worked out once, from all the crates, and never again: a filter only stops drawing a crate,
  // leaving its slot empty, so no box moves.
  let placed = null;
  function layout() {
    if (!placed) placed = computeLayout();
    return placed;
  }

  function computeLayout() {
    const PER_LINE = columns();
    const rows = concerns.layers
      .map((layer) => ({
        layer,
        units: graph.crates
          .filter((c) => c.layer === layer.id && !headOf.has(c.name))
          .map((c) => groupOf(c.name)),
      }))
      .filter((row) => row.units.length);
    const width = PAD_X * 2 + PER_LINE * (BOX_W + GAP_X) - GAP_X;
    const position = new Map();
    // Each box gets its `tier`, the band's line it is on, and its `line`,
    // its level counted down the whole map: a group's crates share a tier
    // and each has a line of its own. `stack` is its place in its group.
    const place = () => {
      let y = PAD_Y;
      let line = 0;
      let tier = 0;
      rows.forEach((row) => {
        row.top = y;
        y += TITLE_H;
        const lines = Math.ceil(row.units.length / PER_LINE);
        const perLine = Math.ceil(row.units.length / lines);
        for (let l = 0; l < lines; l++) {
          const units = row.units.slice(l * perLine, (l + 1) * perLine);
          const depth = Math.max(...units.map((u) => u.length));
          const span = units.length * (BOX_W + GAP_X) - GAP_X;
          const left = (width - span) / 2;
          units.forEach((unit, i) => {
            unit.forEach((name, k) => {
              position.set(name, {
                x: left + i * (BOX_W + GAP_X),
                y: y + k * (BOX_H + STACK_GAP),
                line: line + k,
                tier,
                stack: k,
                depth: unit.length,
              });
            });
          });
          line += depth;
          tier += 1;
          y += depth * BOX_H + (depth - 1) * STACK_GAP + (LINE_H - BOX_H);
        }
        y += 4;
        row.height = y - row.top;
        y += 8;
      });
      return y;
    };
    let height = place();
    const normal = graph.edges;
    for (let sweep = 0; sweep < 6; sweep++) {
      for (const row of rows) {
        // A unit sits over the mean place of its crates' neighbours on
        // other lines of the band, its own crates aside.
        const centre = (unit) => {
          const own = position.get(unit[0]);
          const xs = normal
            .filter((e) => unit.includes(e.from) !== unit.includes(e.to))
            .map((e) => position.get(unit.includes(e.from) ? e.to : e.from))
            .filter((p) => p && p.tier !== own.tier)
            .map((p) => p.x);
          return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : own.x;
        };
        const scores = new Map(row.units.map((u) => [u, centre(u)]));
        row.units.sort((a, b) => scores.get(a) - scores.get(b));
      }
      height = place();
    }
    const groups = [...groupsOf.keys()].map((head) => groupOf(head));
    return { rows, position, groups, width, height: height + PAD_Y };
  }

  // ---- Drawing ----

  let view = null;

  // The filters in the header: each checkbox carries its colour key and
  // explains itself in its tooltip.
  function roleFilters() {
    $("role-filters").replaceChildren(...usedRoles().map((role) => [role, ROLE_ABOUT[role]]).map(([role, about]) =>
      el("label", { class: "role-toggle", title: `${ROLES[role]}: ${about}` },
        el("input", { type: "checkbox", "data-key": `role:${role}`, "data-default": "checked", ...(state.hiddenRoles.has(role) ? {} : { checked: "" }),
          onchange: (ev) => {
            if (ev.target.checked) state.hiddenRoles.delete(role); else state.hiddenRoles.add(role);
            draw();
            renderPanel();
          } }),
        el("span", { class: `role-key role-${role}` }), ROLES[role])));
  }

  // A layer's name on the map is its toggle: a hidden layer's crates are
  // faded and its arrows are not drawn.
  function toggleLayer(id) {
    const hidden = new Set(state.hiddenLayers);
    if (hidden.has(id)) hidden.delete(id); else hidden.add(id);
    state.hiddenLayers = hidden;
    draw();
    renderPanel();
    renderResults();
  }

  // Redrawing replaces the filters and the map, so the element with
  // keyboard focus is found again by its data-key and refocused.
  function keepFocus(rebuild) {
    const active = document.activeElement;
    const key = active && active.getAttribute && active.getAttribute("data-key");
    rebuild();
    if (!key) return;
    const again = document.querySelector(`[data-key="${CSS.escape(key)}"]`);
    if (again && again !== active) again.focus();
  }

  // The Crates and Arrows buttons say how many of their boxes are ticked
  // once any box differs from how the page starts: every crate shown, and
  // arrows used in code (not those only tests or build scripts use).
  function menuCounts() {
    for (const [id, word] of [["crates", "Modules"], ["arrows", "Arrows"]]) {
      const boxes = [...$(`${id}-panel`).querySelectorAll('input[type="checkbox"]')];
      const ticked = boxes.filter((b) => b.checked).length;
      // The boxes built from the state say their start in data-default;
      // the fixed ones in the page, in their checked attribute.
      const start = (b) => (b.dataset.default ? b.dataset.default === "checked" : b.defaultChecked);
      const changed = boxes.some((b) => b.checked !== start(b));
      $(`${id}-button`).textContent = changed ? `${word} (${ticked} of ${boxes.length}) ▾` : `${word} ▾`;
    }
  }

  function kindFilters() {
    const setKinds = (hidden) => {
      state.hiddenKinds = new Set(hidden);
      draw();
      renderPanel();
      renderResults();
    };
    const kinds = (concerns.kinds || []).filter((k) => graph.crates.some((c) => crateKind(c.name) === k.id));
    $("kind-filters").replaceChildren(
      ...kinds.map((k) => el("label", { class: `kind-toggle kind-${kindSlug(k.id)}`, title: `${k.title}: ${k.about}` },
        el("input", { type: "checkbox", "data-key": `kind:${k.id}`, "data-default": "checked", ...(state.hiddenKinds.has(k.id) ? {} : { checked: "" }),
          onchange: (ev) => {
            const hidden = new Set(state.hiddenKinds);
            if (ev.target.checked) hidden.delete(k.id); else hidden.add(k.id);
            setKinds(hidden);
          } }),
        el("span", { class: "kind-key" }), k.title)));
  }

  // The key under the map: colours and drawing conventions only.
  function keyLegend() {
    const kinds = (concerns.kinds || []).filter((k) => graph.crates.some((c) => crateKind(c.name) === k.id));
    $("key").replaceChildren(
      el("span", {}, el("strong", {}, "Kinds: "), kinds.map((k) =>
        el("span", { class: `key-item kind-${kindSlug(k.id)}`, title: k.about }, el("span", { class: "kind-key" }), k.title))),
      el("span", {}, el("strong", {}, "Roles: "), usedRoles().map((role) => [role, ROLES[role]]).map(([role, words]) =>
        el("span", { class: "key-item", title: ROLE_ABOUT[role] }, el("span", { class: `role-key role-${role}` }), words))),
      el("span", { class:"key-item" }, el("span",{class:"route-key"}), "Injected capability"),
      el("span", { class:"muted" }, "Tick a module to see the injections that pass through it."));
  }

  function draw() { keepFocus(drawNow); }

  function drawNow() {
    roleFilters();
    kindFilters();
    menuCounts();
    keyLegend();
    const root = $("graph");
    while (root.childNodes.length > 1) root.removeChild(root.lastChild);
    view = layout();
    view.bandToggles = [];
    root.setAttribute("viewBox", `0 0 ${view.width} ${view.height}`);

    root.append(markers());

    view.rows.forEach((row) => {
      const y = row.top;
      const id = row.layer.id;
      const off = state.hiddenLayers.has(id);
      root.append(svg("rect", { x: 0, y, width: view.width, height: row.height, rx: 10, class: "band" + (off ? " layer-off" : "") }));
      // The layer's name, with an eye that is struck through while the
      // layer is hidden.
      const toggle = svg("g", {
        class: "band-toggle" + (off ? " off" : ""), tabindex: "0", role: "switch", "data-key": `layer:${id}`,
        "aria-checked": off ? "false" : "true", "aria-label": `${row.layer.title} layer's arrows`,
      });
      const tipText = svg("title");
      tipText.textContent = `${row.layer.title}: ${off ? "hidden. Click to show its arrows" : "click to fade its crates and hide their arrows"}`;
      const eye = svg("g", { class: "eye", transform: `translate(12,${y + 7})` });
      eye.append(svg("path", { d: "M0,6 Q7,-1 14,6 Q7,13 0,6 z" }), svg("circle", { cx: 7, cy: 6, r: 2.2 }));
      if (off) eye.append(svg("path", { d: "M1,12 L13,0", class: "slash" }));
      const label = svg("text", { x: 31, y: y + 17, class: "band-title" });
      label.textContent = row.layer.title + (off ? " (hidden)" : "");
      const box = svg("rect", { x: 6, y: y + 3, width: 0, height: 20, rx: 5, class: "band-hit" });
      toggle.append(tipText, box, eye, label);
      toggle.addEventListener("click", (ev) => { ev.stopPropagation(); toggleLayer(id); });
      toggle.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); toggleLayer(id); }
      });
      root.append(toggle);
      view.bandToggles = (view.bandToggles || []).concat([[box, label]]);
    });

    // Each group's dotted outline, under the arrows and boxes. It keeps its
    // full size while any of its crates is drawn, so a partner whose head
    // is filtered out still shows the group it belongs to.
    for (const unit of view.groups) {
      const drawn = unit.filter(drawnCrate);
      if (!drawn.length) continue;
      const top = view.position.get(unit[0]);
      const off = !drawn.some(layerShown);
      const outline = svg("rect", {
        x: top.x - GROUP_PAD, y: top.y - GROUP_PAD, rx: 11,
        width: BOX_W + 2 * GROUP_PAD, height: unit.length * BOX_H + (unit.length - 1) * STACK_GAP + 2 * GROUP_PAD,
        class: "group" + (off ? " layer-off" : ""), "data-group": unit[0],
      });
      const tip = svg("title");
      tip.textContent = `A group: ${unit.slice(1).join(", ")} ${unit.length > 2 ? "implement" : "implements"} a contract of ${unit[0]}, in the same layer`;
      outline.append(tip);
      root.append(outline);
    }

    const edgeLayer = svg("g");
    const hitLayer = svg("g");
    // Arrows, then their wider hover areas, then the boxes on top, so a
    // box always takes the click.
    root.append(edgeLayer, hitLayer);
    view.edgeNodes = [];
    view.hitNodes = [];
    const arrowEnds = placeArrowEnds(graph.edges.filter(visibleEdge));
    for (const [e, [a, b]] of arrowEnds) {
      // One stroke per shown role, side by side, each in its role's colour.
      // One head, on the middle stroke, in the first role's shape: a head
      // per stroke crowds where arrows meet a box. The path starts at the arrowhead. An arrow that
      // shares its end with others leading its run has no head of its own.
      // The stroke with the head runs through the end's point itself, so
      // heads keep the spacing of the ends.
      const roles = shownRoles(e);
      const mid = Math.floor((roles.length - 1) / 2);
      roles.forEach((role, i) => {
        const path = svg("path", {
          d: edgePath(a, b, (i - mid) * STROKE_GAP),
          class: `edge role-${role}`,
          "data-edge": `${e.from}>${e.to}`,
          ...(i === mid
            ? (a.lead ? { "marker-start": `url(#head-${roles[0]})`, "data-head": drawnEnds(e)[1] } : { "data-shared": drawnEnds(e)[1] }) : {}),
        });
        edgeLayer.append(path);
        view.edgeNodes.push({ edge: e, path });
      });
      const hit = svg("path", { d: edgePath(a, b), class: "edge-hit", "data-edge": `${e.from}>${e.to}` });
      hit.addEventListener("mousemove", (ev) => {
        showTip(ev, e);
      });
      hit.addEventListener("mouseleave", hideTip);
      hit.addEventListener("click", () => select(e.from));
      hitLayer.append(hit);
      view.hitNodes.push({ edge: e, hit });
    }

    // Where wired things go, drawn on demand (paintDeliveries), under the
    // boxes.
    view.deliveryLayer = svg("g", { class: "deliveries" });
    root.append(view.deliveryLayer);
    view.nodes = new Map();
    const nodeLayer = svg("g");
    for (const [name, p] of view.position) {
      if (!drawnCrate(name)) continue;
      const c = crates.get(name);
      const g = svg("g", {
        class: "node" +
          (layerShown(name) ? "" : " layer-off") + (programShown(name) ? "" : " program-off") +
          ` kind-${kindSlug(concernOf(name).kind)}`,
        transform: `translate(${p.x},${p.y})`,
        tabindex: "0",
        role: "button",
        "aria-label": `${name}: ${concernOf(name).concern || ""}`,
      });
      g.append(svg("rect", { width: BOX_W, height: BOX_H }));
      const label = svg("text", { x: LABEL_X, y: 17, class: "name" });
      label.textContent = name;
      const sub = svg("text", { x: LABEL_X, y: 32, class: "sub" });
      sub.textContent = subtitle(c);
      g.append(label, sub);
      g.append(pickBox(name));
      if (c.program) g.append(programEye(name));
      g.addEventListener("click", () => select(name));
      g.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); select(name); }
      });
      nodeLayer.append(g);
      view.nodes.set(name, g);
    }
    root.append(nodeLayer);
    // A name or subtitle too long for its box is drawn smaller, whole,
    // clear of the tick box on the box's right edge.
    for (const g of view.nodes.values()) {
      for (const text of g.querySelectorAll("text")) {
        const room = BOX_W - 2 * LABEL_X;
        const wide = text.getComputedTextLength();
        if (wide > room) {
          const size = parseFloat(getComputedStyle(text).fontSize);
          text.style.fontSize = `${(size * room / wide).toFixed(2)}px`;
        }
      }
    }
    // The delivery lines' slot names, above the boxes so none is hidden.
    view.deliveryLabels = svg("g", { class: "delivery-labels" });
    root.append(view.deliveryLabels);
    // Each layer name's focus and hover box fits its text.
    for (const [box, label] of view.bandToggles || []) box.setAttribute("width", label.getBBox().width + 31);
    paint();
  }

  // Which face of each box a line between two boxes meets: "top" or
  // "bottom", or "left" or "right". A line meets a top or bottom face
  // unless another box of the same group lies that way, between the face
  // and the other end: then it meets the side toward the other end, so it
  // never seems to end on a partner. `arc`: both ends on one line, so the
  // line arcs over the boxes between them.
  function facesOf(from, to) {
    const p = view.position.get(from);
    const q = view.position.get(to);
    if (!p || !q) return null;
    const next = (a, b, step) => a.tier === b.tier && a.x === b.x && b.line === a.line + step;
    const face = (a, b) => {
      const side = b.x >= a.x ? "right" : "left";
      if (b.line <= a.line) return a.stack > 0 && !next(a, b, -1) ? side : "top";
      return a.stack < a.depth - 1 && !next(a, b, 1) ? side : "bottom";
    };
    const fa = face(p, q);
    const fb = face(q, p);
    return { fa, fb, arc: p.line === q.line && fa === "top" && fb === "top" };
  }

  // A point `t` of the way along a box's face (left to right, or top to
  // bottom), and the way out of the box there.
  function pointOn(name, face, t) {
    const a = view.position.get(name);
    if (face === "top") return { pt: [a.x + t * BOX_W, a.y], dir: [0, -1] };
    if (face === "bottom") return { pt: [a.x + t * BOX_W, a.y + BOX_H], dir: [0, 1] };
    if (face === "left") return { pt: [a.x, a.y + t * BOX_H], dir: [-1, 0] };
    return { pt: [a.x + BOX_W, a.y + t * BOX_H], dir: [1, 0] };
  }

  // The two ends of a line, each a point on a face with its way out, and
  // how far the curve's control point lies along it.
  function joinEnds(ea, eb, arc) {
    const dx = Math.abs(eb.pt[0] - ea.pt[0]);
    const dy = Math.abs(eb.pt[1] - ea.pt[1]);
    const reach = (e) => {
      if (arc) return 30 + Math.min(40, dx / 8);
      return e.dir[0] ? Math.max(22, dx / 2) : Math.max(12, dy / 2);
    };
    ea.k = reach(ea);
    eb.k = reach(eb);
    return [ea, eb];
  }

  // ---- Spreading the ends along a face ----
  // The lines that meet one face of a box are spread along it, in the
  // order of where their other ends lie, so they cross as little as they
  // can. Ends are END_GAP apart where the face has room, and never less
  // than MIN_GAP, so no two heads touch. A face with more lines than it
  // can hold MIN_GAP apart shares points: its lines are split into runs,
  // the lines of a run end at one point on a shared stem, and only the
  // first of a run is drawn with a head. Lines that end at the box (heads)
  // and lines that leave it are kept in runs of their own, and heads by
  // their role, so one head never stands for two roles.
  const END_GAP = 22;
  const MIN_GAP = 9;
  // How far the ends keep from a face's corners: clear of the tick box and
  // the program's eye, which sit on the right-hand corners.
  const MARGIN = { top: 16, bottom: 16, left: 9, right: 9 };
  const faceLength = (face) => (face === "top" || face === "bottom" ? BOX_W : BOX_H);
  const ROLE_ORDER = ["uses", "wires", "implements", "adapts", "injects", "supplies", "incidental", "other", "unreviewed"];

  // `ends`: [{ order, run }]; gives each its place along the face (0 to
  // 1) and whether it leads its run, in `t` and `lead`.
  function spreadOn(face, ends) {
    const len = faceLength(face);
    const room = len - 2 * MARGIN[face];
    const slots = Math.max(1, Math.floor(room / MIN_GAP) + 1);
    const at = (pos) => pos / len;
    if (ends.length <= slots) {
      ends.sort((a, b) => a.order - b.order);
      const gap = ends.length > 1 ? Math.min(END_GAP, room / (ends.length - 1)) : 0;
      const first = len / 2 - (gap * (ends.length - 1)) / 2;
      ends.forEach((e, i) => { e.t = at(first + i * gap); e.lead = true; });
      return;
    }
    // Too many: each run gets a share of the points, at least one.
    ends.sort((a, b) => a.run - b.run || a.order - b.order);
    const runs = [...new Set(ends.map((e) => e.run))];
    const size = (r) => ends.filter((e) => e.run === r).length;
    const share = new Map(runs.map((r) => [r, 1]));
    let spare = slots - runs.length;
    while (spare > 0) {
      // The next point goes to the run with the most lines per point.
      const r = runs.reduce((best, x) => (size(x) / share.get(x) > size(best) / share.get(best) ? x : best));
      if (size(r) <= share.get(r)) break;
      share.set(r, share.get(r) + 1);
      spare--;
    }
    const used = runs.reduce((n, r) => n + share.get(r), 0);
    const gap = Math.min(END_GAP, room / Math.max(1, used - 1));
    const first = len / 2 - (gap * (used - 1)) / 2;
    let point = 0;
    for (const r of runs) {
      const mine = ends.filter((e) => e.run === r);
      const k = share.get(r);
      mine.forEach((e, j) => {
        const slot = point + Math.floor((j * k) / mine.length);
        e.t = at(first + slot * gap);
        e.lead = j === 0 || Math.floor(((j - 1) * k) / mine.length) !== slot - point;
      });
      point += k;
    }
  }

  // Where the other end of a line lies, along the face: across it for a
  // top or bottom face, down it for a side.
  function orderOf(face, other) {
    const q = view.position.get(other);
    return face === "top" || face === "bottom" ? q.x + BOX_W / 2 + q.y / 1e4 : q.y + BOX_H / 2 + q.x / 1e4;
  }

  // Every drawn arrow's two ends: each face's arrows spread along it.
  // Returns a map from the edge to [its head, its tail],
  // each with `lead`, and keeps each face's points for the delivery lines.
  function placeArrowEnds(edges) {
    const byFace = new Map();
    const add = (name, face, end) => {
      const key = `${name}|${face}`;
      if (!byFace.has(key)) byFace.set(key, { name, face, ends: [] });
      byFace.get(key).ends.push(end);
    };
    const placed = new Map();
    for (const e of edges) {
      const [source, target] = drawnEnds(e);
      const f = facesOf(target, source);
      if (!f) continue;
      const head = { order: orderOf(f.fa, source), run: 1 + ROLE_ORDER.indexOf(shownRoles(e)[0]) };
      const tail = { order: orderOf(f.fb, target), run: 0 };
      add(target, f.fa, head);
      add(source, f.fb, tail);
      placed.set(e, { f, head, tail, target, source });
    }
    for (const { face, ends } of byFace.values()) spreadOn(face, ends);
    view.faceArrows = new Map([...byFace].map(([key, v]) => [key, v.ends.map((x) => x.t).sort((a, b) => a - b)]));
    const out = new Map();
    for (const [e, { f, head, tail, target, source }] of placed) {
      const ends = joinEnds(pointOn(target, f.fa, head.t), pointOn(source, f.fb, tail.t), f.arc);
      ends[0].lead = head.lead;
      out.set(e, ends);
    }
    return out;
  }

  // The delivery lines' ends: on each face, in the spaces between the
  // arrows' ends there, in the order of their other ends; on a face with
  // no arrows, spread as arrows are.
  function placeDeliveryEnds(lines) {
    const byFace = new Map();
    const placed = [];
    for (const line of lines) {
      const f = facesOf(line.program, line.receiver);
      if (!f) continue;
      const a = { order: orderOf(f.fa, line.receiver), run: 0 };
      const b = { order: orderOf(f.fb, line.program), run: 0 };
      for (const [name, face, end] of [[line.program, f.fa, a], [line.receiver, f.fb, b]]) {
        const key = `${name}|${face}`;
        if (!byFace.has(key)) byFace.set(key, { face, ends: [] });
        byFace.get(key).ends.push(end);
      }
      placed.push({ line, f, a, b });
    }
    for (const [key, { face, ends }] of byFace) {
      // Each point once: arrows sharing a point leave no space between them.
      const arrows = [...new Set((view.faceArrows && view.faceArrows.get(key)) || [])];
      if (!arrows.length) { spreadOn(face, ends); continue; }
      const len = faceLength(face);
      const bounds = [MARGIN[face] / len, ...arrows, 1 - MARGIN[face] / len];
      const spaces = bounds.slice(1).map((t, i) => (t + bounds[i]) / 2);
      // In order: one space each while they fit, sharing points, on shared
      // stems as arrows do, when they do not.
      ends.sort((x, y) => x.order - y.order);
      ends.forEach((e, i) => {
        const pick = ends.length >= spaces.length ? Math.floor((i * spaces.length) / ends.length)
          : Math.round(((i + 0.5) * spaces.length) / ends.length - 0.5);
        e.t = spaces[Math.min(spaces.length - 1, pick)];
      });
    }
    return placed.map(({ line, f, a, b }) => ({
      line, ends: joinEnds(pointOn(line.program, f.fa, a.t), pointOn(line.receiver, f.fb, b.t), f.arc),
    }));
  }

  // One arrowhead per role: a filled arrow for uses, a hollow triangle for
  // implementing a contract (as in UML), an open arrow for wiring.
  function markers() {
    const defs = svg("defs");
    const shapes = {
      supplies: ["M0,0 L8,4 L0,8 z", "head-supplies"],
      uses: ["M0,0 L8,4 L0,8 z", "head-uses"],
      implements: ["M0.5,0.5 L8,4 L0.5,7.5 z", "head-implements"],
      adapts: ["M0.5,4 a3.5,3.5 0 1,0 7,0 a3.5,3.5 0 1,0 -7,0 z", "head-adapts"],
      wires: ["M0,0 L8,4 L0,8", "head-wires"],
      injects: ["M0,4 L4,0.5 L8,4 L4,7.5 z", "head-injects"],
      incidental: ["M0,0 L8,4 L0,8", "head-incidental"],
      other: ["M0,0 L8,4 L0,8 z", "head-other"],
      unreviewed: ["M0,0 L8,4 L0,8 z", "head-unreviewed"],
    };
    for (const [role, [d, cls]] of Object.entries(shapes)) {
      const marker = svg("marker", {
        id: `head-${role}`, viewBox: "-1 -1 10 10", refX: "7.5", refY: "4", markerWidth: "9", markerHeight: "9",
        markerUnits: "userSpaceOnUse",
        orient: "auto-start-reverse",
      });
      marker.append(svg("path", { d, class: cls }));
      defs.append(marker);
    }
    return defs;
  }

  // The cubic curve a line follows: start, two control points, end, each
  // control point out from its end along the way the line leaves it.
  function edgeCurve(a, b) {
    const out = (e) => [e.pt[0] + e.dir[0] * e.k, e.pt[1] + e.dir[1] * e.k];
    return [a.pt, out(a), out(b), b.pt];
  }

  // The arrow's path. With `off`, the path runs parallel to the curve at
  // that distance, measured along the curve's own perpendicular at every
  // point, so the strokes of an arrow with several roles stay apart on
  // shallow arrows and arcs too.
  function edgePath(a, b, off = 0) {
    const [p0, p1, p2, p3] = edgeCurve(a, b);
    if (!off) return `M${p0[0]},${p0[1]} C${p1[0]},${p1[1]} ${p2[0]},${p2[1]} ${p3[0]},${p3[1]}`;
    const at = (t, i) => {
      const u = 1 - t;
      return u * u * u * p0[i] + 3 * u * u * t * p1[i] + 3 * u * t * t * p2[i] + t * t * t * p3[i];
    };
    const slope = (t, i) => {
      const u = 1 - t;
      return 3 * u * u * (p1[i] - p0[i]) + 6 * u * t * (p2[i] - p1[i]) + 3 * t * t * (p3[i] - p2[i]);
    };
    const points = [];
    let normal = [1, 0];
    for (let k = 0; k <= EDGE_STEPS; k++) {
      const t = k / EDGE_STEPS;
      const dx = slope(t, 0);
      const dy = slope(t, 1);
      const len = Math.hypot(dx, dy);
      if (len > 1e-6) normal = [-dy / len, dx / len];
      points.push(`${(at(t, 0) + normal[0] * off).toFixed(1)},${(at(t, 1) + normal[1] * off).toFixed(1)}`);
    }
    return `M${points.join(" L")}`;
  }

  function subtitle(c) {
    return c.kind === "composition" ? "program · composition" : c.kind;
  }

  // The checkbox in a box's top corner: adds the crate to the selection,
  // or takes it out. Its hit area is larger than the box it draws, for a
  // finger at phone width.
  function pickBox(name) {
    const box = svg("g", { class: "pick", role: "checkbox", tabindex: "0", "data-key": `pick:${name}`,
      "aria-checked": state.checked.has(name) ? "true" : "false", "aria-label": `Highlight ${name}` });
    const tip = svg("title");
    tip.textContent = `Light up ${name}'s arrows and neighbours, with any other ticked crate's`;
    // Centred on the box's top right corner, clear of its name.
    box.append(tip, svg("rect", { class: "pick-hit", x: BOX_W - 20, y: -17, width: 34, height: 34 }),
      svg("rect", { class: "pick-box", x: BOX_W - 6, y: -6, width: 12, height: 12, rx: 2 }),
      svg("path", { class: "pick-tick", d: `M${BOX_W - 3.5},0 l2.5,2.5 l4.5,-5` }));
    const toggle = (ev) => { ev.stopPropagation(); ev.preventDefault(); togglePick(name); };
    box.addEventListener("click", toggle);
    box.addEventListener("keydown", (ev) => { if (ev.key === "Enter" || ev.key === " ") toggle(ev); });
    return box;
  }

  // The eye in a program box's bottom corner: hides or shows the program.
  function programEye(name) {
    const off = state.hiddenPrograms.has(name);
    const eye = svg("g", { class: "prog-eye" + (off ? " off" : ""), role: "switch", tabindex: "0", "data-key": `eye:${name}`,
      "aria-checked": off ? "false" : "true", "aria-label": `${name} shown` });
    const tip = svg("title");
    tip.textContent = off ? `${name} is hidden: show it` : `Hide ${name}: fade it and hide its arrows`;
    // Centred on the box's bottom right corner, on a disc of its own.
    const x = BOX_W - 7;
    const y = BOX_H;
    eye.append(tip, svg("rect", { class: "eye-hit", x: BOX_W - 20, y: BOX_H - 17, width: 34, height: 34 }),
      svg("circle", { class: "eye-back", cx: BOX_W, cy: y, r: 9 }),
      svg("path", { class: "eye-shape", d: `M${x},${y} Q${x + 7},${y - 6} ${x + 14},${y} Q${x + 7},${y + 6} ${x},${y} z` }),
      svg("circle", { class: "eye-pupil", cx: BOX_W, cy: y, r: 1.8 }));
    if (off) eye.append(svg("path", { class: "eye-slash", d: `M${x + 1},${y + 6} L${x + 13},${y - 6}` }));
    const toggle = (ev) => { ev.stopPropagation(); ev.preventDefault(); toggleProgram(name); };
    eye.addEventListener("click", toggle);
    eye.addEventListener("keydown", (ev) => { if (ev.key === "Enter" || ev.key === " ") toggle(ev); });
    return eye;
  }

  // Ticking highlights; it leaves the panel as it is, apart from the list
  // of ticked crates.
  function togglePick(name) {
    if (state.checked.has(name)) state.checked.delete(name); else state.checked.add(name);
    paint();
    renderPanel();
  }

  function toggleProgram(name) {
    if (state.hiddenPrograms.has(name)) state.hiddenPrograms.delete(name); else state.hiddenPrograms.add(name);
    draw();
    renderPanel();
    renderResults();
  }

  // Classes for the ticked crates' neighbourhood, the panel's crate and the
  // search hits. The highlight is the union over the ticked crates; with none
  // ticked, every arrow is faint.
  function paint() {
    if (!view) return;
    const focus = state.checked;
    const deps = new Set();
    const users = new Set();
    for (const { edge, path } of view.edgeNodes) {
      path.classList.remove("focus", "faint");
      if (focus.has(edge.from) || focus.has(edge.to)) {
        path.classList.add("focus");
        if (focus.has(edge.from)) deps.add(edge.to);
        if (focus.has(edge.to)) users.add(edge.from);
      } else {
        path.classList.add("faint");
      }
    }
    // A faded arrow's hover area takes no pointer: no tooltip, and it does
    // not cover the full-strength arrows and boxes under it.
    for (const { edge, hit } of view.hitNodes) {
      hit.classList.toggle("faint", !(focus.has(edge.from) || focus.has(edge.to)));
    }
    const hits = searchHits();
    for (const [name, node] of view.nodes) {
      node.classList.toggle("checked", focus.has(name));
      node.classList.toggle("current", name === state.selected);
      node.classList.toggle("dep", deps.has(name));
      node.classList.toggle("user", users.has(name));
      node.classList.toggle("hit", hits.crates.has(name));
      const dim = (focus.size && !focus.has(name) && !deps.has(name) && !users.has(name)) ||
        (!focus.size && state.query && !hits.crates.has(name));
      node.classList.toggle("dim", Boolean(dim));
      const pick = node.querySelector(".pick");
      if (pick) pick.setAttribute("aria-checked", focus.has(name) ? "true" : "false");
    }
    paintDeliveries();
  }

  // A route's project modules, origin first; an injection has at least three:
  // the supplier, the program that wires it, and the receiver at the end.
  function routeNames(r) { return r.modules; }
  const supplierOf = (r) => r.modules[0];
  const programOf = (r) => r.modules[1];
  const receiverOf = (r) => r.modules[r.modules.length - 1];
  function paintDeliveries() {
    const layer = view.deliveryLayer, labelLayer = view.deliveryLabels;
    layer.replaceChildren(); labelLayer.replaceChildren();
    for (const {path} of view.edgeNodes) path.classList.remove("route-hop");
    const active = new Map();
    for (const route of graph.routes) {
      // Other routes are described in the panel. Only injections add lines to the graph.
      if (!route.injection) continue;
      const names = routeNames(route);
      if (!names.some(n=>state.checked.has(n)) || !names.every(visibleCrate)) continue;
      if (state.hiddenRoles.has(route.first_role)) continue;
      const strokes = view.edgeNodes.filter(({edge})=>edge.from===programOf(route) && edge.to===supplierOf(route));
      const hop = strokes.find(({path})=>path.classList.contains("role-"+route.first_role));
      if (!hop) continue;
      hop.path.classList.remove("faint"); hop.path.classList.add("route-hop");
      for (const n of names) view.nodes.get(n).classList.remove("dim");
      const leg = programOf(route) + ">" + receiverOf(route);
      if (!active.has(leg)) active.set(leg,{program:programOf(route), receiver:receiverOf(route), routes:[]});
      active.get(leg).routes.push(route);
    }
    const LABEL_CLEAR = 5;
    const placed = [...view.nodes.keys()].map((n) => view.position.get(n)).filter(Boolean)
      .map((q) => ({ x: q.x - LABEL_CLEAR, y: q.y - LABEL_CLEAR, w: BOX_W + 2 * LABEL_CLEAR, h: BOX_H + 2 * LABEL_CLEAR }))
      .concat(view.groups.map((unit) => {
        const q = view.position.get(unit[0]);
        return { x: q.x - GROUP_PAD, y: q.y - GROUP_PAD, w: BOX_W + 2 * GROUP_PAD,
          h: unit.length * BOX_H + (unit.length - 1) * STACK_GAP + 2 * GROUP_PAD };
      }));
    const overlaps = (r) => placed.some((q) => r.x < q.x + q.w && q.x < r.x + r.w && r.y < q.y + q.h && q.y < r.y + r.h);
    // A label on its own line: the stretch nearest the middle where it
    // covers no box and no other label, trying the whole length, just
    // above the line, then just below. Where every stretch crosses a box,
    // it sits mid-line on a backing of the page's colour, so it reads
    // over the box.
    const labelOn = (path, from, to, words, extra, tip) => {
      const label = svg("text", { class: "delivery-label" + extra, "data-from": from, "data-to": to });
      label.textContent = words;
      label.addEventListener("mousemove", tip);
      label.addEventListener("mouseleave", hideTip);
      labelLayer.append(label);
      const len = path.getTotalLength();
      const spots = [];
      for (let k = 0; k <= 9; k++) {
        for (const f of k ? [0.5 - k * 0.05, 0.5 + k * 0.05] : [0.5]) {
          for (const dy of [0, 15]) spots.push([f, dy]);
        }
      }
      const at = (f, dy) => {
        const pt = path.getPointAtLength(len * f);
        label.setAttribute("x", pt.x + 4);
        label.setAttribute("y", pt.y - 3 + dy);
        const box = label.getBBox();
        return { x: box.x - 2, y: box.y - 1, w: box.width + 4, h: box.height + 2 };
      };
      const free = spots.find(([f, dy]) => !overlaps(at(f, dy)));
      const r = free ? at(...free) : at(0.5, 0);
      placed.push(r);
      if (!free) {
        const back = svg("rect", { class: "delivery-label-back", x: r.x, y: r.y, width: r.w, height: r.h, rx: 3 });
        labelLayer.insertBefore(back, label);
      }
    };

    for (const {line,ends:[a,b]} of placeDeliveryEnds([...active.values()])) {
      const r=line.routes[0];
      const path=svg("path",{d:edgePath(a,b),class:"delivery","data-from":line.program,"data-to":line.receiver,"marker-end":"url(#head-wires)"});
      path.addEventListener("mousemove",ev=>showRouteTip(ev,line.routes));
      path.addEventListener("mouseleave",hideTip);
      layer.append(path);
      labelOn(path,line.program,line.receiver,r.carry,"",ev=>showRouteTip(ev,line.routes));
    }
  }
  function tooltip(ev, ...content) {
    const tip=$("tip"); tip.replaceChildren(...content); tip.hidden=false;
    tip.style.left=Math.max(8,Math.min(ev.clientX+14,innerWidth-tip.offsetWidth-8))+"px";
    tip.style.top=Math.max(8,Math.min(ev.clientY+14,innerHeight-tip.offsetHeight-8))+"px";
  }
  function showRouteTip(ev,routes) {
    tooltip(ev,...routes.flatMap(r=>[el("strong",{},r.id),el("div",{},routeNames(r).join(" → ")),el("div",{},r.review.reason||reviewWords(r.review))]));
  }
  function showTip(ev,e) {
    tooltip(ev,el("strong",{},drawnEnds(e).join(" → ")),el("div",{},roleWords(e)),
      el("div",{},e.detail||reviewWords(e.review)));
  }
  function hideTip() { $("tip").hidden=true; }

  // Clicking a box shows that crate in the panel; null shows the overview.
  // It highlights nothing and ticks nothing.
  function select(name) {
    state.selected = name;
    paint();
    renderPanel();
    try { history.replaceState(null, "", name ? `#${name}` : "#"); } catch (_) { /* file:// may refuse */ }
  }

  // The ticked crates, at the top of the panel: each opens its own panel,
  // and one button clears them.
  function selectionList() {
    if (!state.checked.size) return null;
    return el("div", { class: "selection-list" },
      el("strong", {}, `Highlighted (${state.checked.size}): `),
      [...state.checked].map((n, i) => [i ? ", " : "",
        el("button", { class: "crate-link", type: "button", "aria-current": n === state.selected ? "true" : "false",
          onclick: () => { state.selected = n; paint(); renderPanel(); } }, n)]),
      " ", el("button", { class: "clear-selection", type: "button", onclick: () => { state.checked = new Set(); paint(); renderPanel(); } }, "Clear"));
  }

  // ---- Review and evidence ----

  const REVIEW_WORDS = { judged: "Judged", deferred: "Deferred", unreviewed: "Unreviewed" };
  function reviewWords(review) {
    return review.status === "deferred" ? `Deferred: ${review.reason}` : REVIEW_WORDS[review.status];
  }
  // An arrow's or route's roles in words, naming any role the reviewer named.
  // `words` is ROLES for an arrow's captions, ROLE_NAMES for a route.
  function roleWords(subject, words = ROLES) {
    let other = 0;
    return subject.roles.map((r) => r === "other" ? `${subject.other_roles[other++] || "other"}` : words[r]).join(" · ");
  }
  function reviewBadge(review) {
    if (review.status === "judged") return null;
    return el("span", { class: `badge review-badge review-${review.status}`, title: review.reason || "" }, REVIEW_WORDS[review.status]);
  }
  const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
  const locationText = (l) => (l ? `${l.file} · bytes ${l.start}–${l.end}` : null);
  function evidenceEntry(id, mark) {
    const e = graph.evidence[id];
    if (!e) return el("li", { class: "empty" }, `Missing evidence: ${id}`);
    const where = [locationText(e.location), e.unit ? `build unit ${e.unit}` : null].filter(Boolean).join(" · ");
    return el("li", {}, mark ? el("span", { class: "badge" }, mark) : null, " ", prose(e.summary),
      el("div", { class: "muted mono evidence-where" }, where || "No source location"));
  }
  // A review: its decision, its reason, and the evidence it cites.
  function reviewBody(review) {
    if (review.status === "unreviewed") return el("p", { class: "empty" }, "No reviewer has judged this yet.");
    if (review.status === "deferred") return el("p", {}, el("strong", {}, "Deferred. "), review.reason);
    return el("div", {}, el("p", {}, review.reason),
      el("ul", { class: "evidence" }, evidenceEntry(review.primary, "primary"), review.supporting.map((id) => evidenceEntry(id, null))));
  }
  // A route's hops in order, each with its support and evidence, then its end.
  function hopList(r) {
    return el("ol", { class: "hops" },
      el("li", {}, el("strong", {}, r.origin.label), el("span", { class: "muted" }, " origin")),
      r.hops.map((h) => el("li", {},
        el("strong", {}, h.to.label),
        h.role ? el("span", { class: "muted" }, ` · ${ROLE_NAMES[h.role] || h.role}`) : null,
        h.established ? null : el("span", { class: "badge possible" }, h.condition ? `possible: ${h.condition}` : "possible"),
        el("ul", { class: "evidence" }, evidenceEntry(h.evidence, null)))),
      r.end.reached ? null : el("li", { class: "stopped" }, el("strong", {}, "Stopped. "), r.end.reason,
        el("ul", { class: "evidence" }, evidenceEntry(r.end.at, null))));
  }

  function moduleLink(n) {
    return el("button",{class:"crate-link",type:"button",onclick:()=>select(n)},n);
  }
  function routeList(routes) {
    return el("ul",{class:"flows"},routes.map(r=>el("li",{},
      el("div",{class:"item-head"},el("strong",{},r.id),r.roles.length?el("span",{class:"muted"},roleWords(r,ROLE_NAMES)):null,reviewBadge(r.review)),
      el("div",{},routeNames(r).map((n,i)=>[i?" → ":"",moduleLink(n)])),
      r.review.status==="judged"?el("p",{},r.review.reason):el("p",{class:"muted"},reviewWords(r.review)),
      el("details",{class:"route-detail"},el("summary",{},`Hops and evidence (${r.hops.length})`),hopList(r),
        r.review.status==="judged"?el("div",{},el("h4",{},"Review evidence"),reviewBody(r.review)):null))));
  }
  function renderPanel() {
    const panel=$("panel"), n=state.selected, highlighted=selectionList();
    if (!n) {
      const count=(status)=>graph.crates.filter(c=>c.review.status===status).length+graph.edges.filter(e=>e.review.status===status).length+graph.routes.filter(r=>r.review.status===status).length;
      const open=["unreviewed","deferred"].map(s=>[s,count(s)]).filter(([,k])=>k);
      panel.replaceChildren(...[highlighted,el("h2",{},graph.meta.project),
        el("p",{class:"muted"},`${plural(graph.crates.length,"module")}, ${graph.edges.length} ${graph.edges.length===1?"dependency":"dependencies"} and ${plural(graph.routes.length,"route")}.`),
        el("ul",{class:"overview"},
          el("li",{},el("strong",{},"Source revision. "),el("span",{class:"mono"},graph.meta.revision)),
          el("li",{},el("strong",{},"Target. "),graph.meta.target),
          el("li",{},el("strong",{},"Configuration. "),graph.meta.configuration),
          el("li",{},el("strong",{},"Graph. "),el("span",{class:"mono"},graph.meta.graph_id))),
        open.length?el("p",{},"Awaiting review: ",open.map(([s,k],i)=>[i?", ":"",`${k} ${REVIEW_WORDS[s].toLowerCase()}`]),"."):null,
        el("ul",{class:"overview"},concerns.layers.map(l=>el("li",{},el("strong",{},l.title+". "),
          plural(graph.crates.filter(c=>c.layer===l.id).length,"module")))),
        section("gaps","Coverage gaps",graph.gaps.length,()=>el("ul",{class:"evidence"},graph.gaps.map(g=>
          el("li",{},prose(g.summary),el("div",{class:"muted mono evidence-where"},[locationText(g.location),g.unit?`build unit ${g.unit}`:null].filter(Boolean).join(" · ")||"No source location"))))),
        el("p",{}, "Select a module for its surface, review and routes. Tick its corner to highlight its dependencies.")].filter(Boolean));
      return;
    }
    const c=crates.get(n), routes=graph.routes.filter(r=>routeNames(r).includes(n));
    const neighbors=(edges,other)=>el("ul",{class:"flows"},edges.map(e=>el("li",{},
      el("div",{class:"item-head"},moduleLink(e[other]),el("span",{class:"muted"},roleWords(e)),reviewBadge(e.review)),
      el("div",{},e.detail))));
    const includes=graph.edges.filter(e=>e.from===n), users=graph.edges.filter(e=>e.to===n);
    const surface=()=>el("ul",{class:"items"},c.surface.map(item=>el("li",{},
      el("div",{class:"item-head"},el("span",{class:"kind"},item.kind),el("code",{},item.name)),
      item.signature?el("div",{class:"item-doc"},el("code",{},item.signature)):null,
      item.location?el("div",{class:"item-doc mono"},locationText(item.location)):null)));
    panel.replaceChildren(...[el("h2",{},c.name),highlighted,
      el("div",{class:"badges"},el("span",{class:"badge kind-badge kind-"+kindSlug(c.kind)},"kind: "+c.kind),el("span",{class:"badge"},layerTitle(c.layer)),reviewBadge(c.review)),
      c.concern?el("p",{},c.concern):null, section("surface","Surface",c.surface.length,surface),
      section("review","Review",1,()=>reviewBody(c.review)),
      section("wiring","Routes",routes.length,()=>routeList(routes)),
      section("includes","Depends on",includes.length,()=>neighbors(includes,"to")),
      section("included-in","Used by",users.length,()=>neighbors(users,"from"))].filter(Boolean));
    panel.scrollTop=0;
  }

  // A panel section the viewer opens and closes, with a count beside its
  // heading. An empty section is not shown. It starts open when it is
  // short (SECTION_OPEN_MAX entries or fewer) and closed when long; each
  // viewer's choice is kept per section, in this browser only, and the
  // page works the same without it. `body` is built only when shown.
  const SECTION_OPEN_MAX = 5;
  const SECTION_KEY = "sharsheret-section:";
  function sectionChoice(id) {
    try { return localStorage.getItem(SECTION_KEY + id); } catch (_) { return null; }
  }
  function keepSectionChoice(id, open) {
    try { localStorage.setItem(SECTION_KEY + id, open ? "open" : "closed"); } catch (_) { /* no storage: not kept */ }
  }
  function section(id, title, count, body, about, forceOpen) {
    if (!count) return null;
    const choice = sectionChoice(id);
    const open = forceOpen || (choice ? choice === "open" : count <= SECTION_OPEN_MAX);
    const details = el("details", { class: "section", "data-section": id, ...(open ? { open: "" } : {}) },
      el("summary", {}, el("h3", about ? { title: about } : {}, title, " ", el("span", { class: "count" }, `(${count})`))));
    if (open) details.append(body());
    // A section drawn open fires `toggle` too, so only a toggle that
    // follows the viewer's click on the heading is kept as their choice.
    let chosen = false;
    details.querySelector("summary").addEventListener("click", () => { chosen = true; });
    details.addEventListener("toggle", () => {
      if (chosen) keepSectionChoice(id, details.open);
      chosen = false;
      if (details.open && details.children.length === 1) details.append(body());
    });
    return details;
  }

  // ---- Search ----

  function searchHits() {
    const q = state.query;
    const result = { crates: new Set(), items: [] };
    if (!q) return result;
    for (const c of graph.crates) {
      const texts=[c.name,c.concern,c.review.reason||"",...c.surface.map(i=>i.name)];
      if (texts.some(t=>t.toLowerCase().includes(q))) result.crates.add(c.name);
    }
    return result;
  }

  function renderResults() {
    const box = $("results");
    if (!state.query) { box.hidden = true; box.replaceChildren(); return; }
    const hits = searchHits();
    const rows=[...hits.crates].map(n=>el("button",{type:"button",onclick:()=>{ select(n); clearSearch(); }},el("strong",{},n)," ",el("span",{class:"muted"},crateKind(n)),hiddenNote(n)));
    box.replaceChildren(...(rows.length ? rows : [el("div", { class: "empty" }, "Nothing matches.")]));
    box.hidden = false;
  }

  // A search hit or a selected crate whose kind is filtered out says so,
  // rather than pointing at nothing on the map.
  // Why a crate is not drawn, if a filter hides it.
  function hiddenWhy(name) {
    const why = [];
    if (!kindShown(name)) why.push(`its kind, ${crateKind(name)}, is hidden`);
    return why.join(", and ");
  }
  const layerTitle = (id) => (concerns.layers.find((l) => l.id === id) || {}).title || id;

  function hiddenNote(name) {
    const why = hiddenWhy(name);
    if (why) return el("span", { class: "hidden-kind" }, ` (not drawn: ${why})`);
    return layerShown(name) ? null : el("span", { class: "hidden-kind" }, ` (layer hidden: no arrows)`);
  }

  function clearSearch() {
    $("search").value = "";
    state.query = "";
    renderResults();
    paint();
  }

  // ---- Controls ----

  document.title = `${graph.meta.project} module map`;
  $("map-title").textContent = document.title;
  $("meta").textContent = [graph.meta.revision, graph.meta.target, graph.meta.configuration].join(" · ");

  $("search").addEventListener("input", (ev) => {
    state.query = ev.target.value.trim().toLowerCase();
    renderResults();
    paint();
  });
  $("search").addEventListener("keydown", (ev) => { if (ev.key === "Escape") clearSearch(); });
  // The Crates and Arrows menus: a button opens its list; Escape, a click
  // outside, or focus leaving it closes it. One is open at a time.
  const menus = ["crates", "arrows"];
  let pointerDown = false;
  document.addEventListener("pointerdown", () => { pointerDown = true; }, true);
  document.addEventListener("pointerup", () => { setTimeout(() => { pointerDown = false; }, 0); }, true);
  function setMenu(id, open, focusInside) {
    const button = $(`${id}-button`);
    $(`${id}-panel`).hidden = !open;
    button.setAttribute("aria-expanded", open ? "true" : "false");
    if (open) {
      for (const other of menus) if (other !== id) setMenu(other, false);
      if (focusInside) { const first = $(`${id}-panel`).querySelector("input, button"); if (first) first.focus(); }
    }
  }
  for (const id of menus) {
    const button = $(`${id}-button`);
    const menu = $(`${id}-menu`);
    button.addEventListener("click", (ev) => setMenu(id, $(`${id}-panel`).hidden, ev.detail === 0));
    menu.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape" && !$(`${id}-panel`).hidden) { ev.preventDefault(); setMenu(id, false); button.focus(); }
    });
    // Only keyboard focus leaving closes it here. A press of the mouse
    // closes it through the click instead: closing on the press would move
    // the other menu's button on a phone, where the list pushes it down,
    // before the click lands.
    menu.addEventListener("focusout", (ev) => {
      if (!pointerDown && ev.relatedTarget && !menu.contains(ev.relatedTarget)) setMenu(id, false);
    });
    $(`${id}-panel`).addEventListener("change", menuCounts);
  }
  // All and None at the top of each menu tick or untick every box in it.
  const shownKinds = () => (concerns.kinds || []).filter((k) => graph.crates.some((c) => crateKind(c.name) === k.id)).map((k) => k.id);
  function redrawAll() {
    draw();
    renderPanel();
    renderResults();
  }
  function setCrates(all) {
    state.hiddenKinds = new Set(all ? [] : shownKinds());
    redrawAll();
  }
  function setArrows(all) {
    state.hiddenRoles = new Set(all ? [] : Object.keys(ROLE_ABOUT));
    redrawAll();
  }
  $("crates-all").addEventListener("click", () => setCrates(true));
  $("crates-none").addEventListener("click", () => setCrates(false));
  $("arrows-all").addEventListener("click", () => setArrows(true));
  $("arrows-none").addEventListener("click", () => setArrows(false));
  // Every layer at once; one layer's name still toggles just that layer.
  function setLayers(shown) {
    state.hiddenLayers = new Set(shown ? [] : concerns.layers.map((l) => l.id));
    // Show all brings hidden programs back too.
    if (shown) state.hiddenPrograms = new Set();
    redrawAll();
  }
  $("layers-show").addEventListener("click", () => setLayers(true));
  $("layers-hide").addEventListener("click", () => setLayers(false));
  // Reset view: every filter, menu and layer as the page starts, the search
  // cleared and no crate selected. The map shown stays.
  $("reset-view").addEventListener("click", () => {
    state.hiddenKinds = new Set();
    state.hiddenRoles = new Set();
    state.hiddenLayers = new Set();
    state.hiddenPrograms = new Set();
    for (const box of document.querySelectorAll(".menu-panel input[type=checkbox]")) box.checked = box.defaultChecked;
    for (const id of menus) setMenu(id, false);
    $("search").value = "";
    state.query = "";
    draw();
    renderResults();
    state.checked = new Set();
    select(null);
  });

  // The path is taken at the click, since a redraw may already have
  // replaced the element clicked.
  document.addEventListener("click", (ev) => {
    const path = ev.composedPath();
    for (const id of menus) if (!path.includes($(`${id}-menu`))) setMenu(id, false);
  });

  $("graph").addEventListener("click", (ev) => {
    if (ev.target === $("graph") || ev.target.classList.contains("band")) select(null);
  });

  // A link to `#crate` selects that crate, also from the address bar.
  window.addEventListener("hashchange", () => {
    const name = resolveHash();
    if (crates.has(name) && name !== state.selected) select(name);
  });
  function resolveHash() { return decodeURIComponent(location.hash.slice(1)); }
  const initial = resolveHash();
  draw();
  window.matchMedia("(max-width: 600px)").addEventListener("change",()=>{ placed=null; draw(); });
  if (initial && crates.has(initial)) select(initial);
  else renderPanel();
})();
