# Plan: draft maps

## Goal

Make `map-web` useful for planning as well as reading. A map can hold proposed modules, dependencies and routes beside observed ones, or consist only of proposed parts. A person or agent edits a draft as a file. A preview in the browser reloads when the file changes.

Sam's decisions, which this plan takes as given:

- `ProjectMap` marks each module, dependency and route as observed in code or proposed. `map-web` draws proposed parts distinctly.
- Drafts are edited only as files. There is no editing in the browser.
- The file format belongs to `map-persist`.
- The program gains a `shrsh draft` subcommand. The work itself lives in separate crates.

## 1. Changes to `project-map`

Each change below breaks the current contract. Today the only consumers are `map-web` and the `example` module, and both are updated in the same step. The PR description should call out the change.

| Change | Why |
| --- | --- |
| Add `status: Status` to `MapModule`, `MapDependency` and `MapRoute`, with `enum Status { Observed, Proposed }`. | This is Sam's decision. Each part says which it is. |
| Replace the four source fields of `ProjectMap` (`graph_id`, `source_revision`, `target`, `configuration`) with `source: Option<MapSource>`. | A map made only of proposals has no graph or build. A sentinel value such as `"none"` would hide that. |
| Add `HopSupport::Proposed`. | A proposed hop is neither established nor possible in code. |
| Add `kind: EvidenceKind` to `MapEvidence`, with `enum EvidenceKind { Source, Proposal }`. | Proposed parts still need something to cite in `Review::Judged` and on each hop. A proposal note is the author's reasoning, not something found in the source. The viewer must show the two differently. `Review` and hops otherwise stay as they are. |
| Add `ProjectMap::problems(&self) -> Vec<MapProblem>`, a pure consistency check. | Hand-written maps need checks that `map-builder` guarantees for generated maps. |

`problems` reports:

- A citation that is not in `evidence`.
- A route hop whose dependency, as given by `hop_dependency`, is not in `dependencies`.
- An observed part that rests on a proposed part. Examples are an observed dependency that touches a proposed module, or an observed route that cites a proposal note or passes through a proposed module.
- A proposed hop in an observed route, or an observed hop in a proposed route.
- A module layer that is not in `layers`.

Proposed parts may point at observed parts. For example, a proposed route may run through observed modules.

## 2. `map-persist`: the file format

`map-persist` owns one format for both saved maps and drafts.

- **Encoding: TOML.** It allows comments, reads well when edited by hand, and maps keyed tables onto the map's `BTreeMap`s. The `toml` crate is well maintained. JSON has no comments. YAML's Rust libraries are poorly maintained.
- **Its own document types.** `map-persist` defines serde types for the file and converts them to and from `project_map` types. `project-map` gains no serde dependency, and the file can change version without changing the contract.
- **A version line.** Each file starts with `format = 1`. Loading rejects an unknown version.
- **No implicit defaults.** The format document lists every field that may be left out and the value it takes. In a draft, a part's `status` may be omitted and defaults to `proposed`. In a saved map, `status` is required.
- **Contract.** `load(text) -> Result<MapDocument, LoadError>` and `save(&MapDocument) -> String`. `LoadError` carries a line and column. Reading and writing files is the caller's job.
- **Two document kinds:**
  - A **map**: a complete `ProjectMap`. `mapper` writes these after analysis.
  - A **draft**: optional `base = "<path to a saved map>"`, then the proposed parts and their notes. Without a base, the draft is a map made only of proposals.

A draft that lays changes over a base map, rather than copying the whole map, stays small. When analysis reruns and the base map is saved again, the proposals stay attached to the new map.

## 3. New crates for drafts and the preview

| Crate | Concern | Effects |
| --- | --- | --- |
| `map-draft` | Combines a base map and a draft into one `ProjectMap`. Reports problems: an ID in the draft that collides with the base, a reference to a missing base part, and everything `ProjectMap::problems` finds. | None. It receives values, not paths. |
| `map-preview` | Serves a page on localhost and tells the browser to reload when told the map has changed. Shows the last good map with a list of problems when a reload fails. | Network. It wraps the HTTP server and the file watcher behind its own contracts, so those libraries can be replaced. |

The preview serves a small outer page. That page holds `map-web`'s page in a frame and listens for reload events from the server (Server-Sent Events). It also shows the problem list. Because of this, `map-web`'s page keeps its current promise of no network requests.

## 4. `shrsh draft`

`mapper`, the program, gains a `shrsh` binary with one subcommand group to start with. `mapper` only composes the pieces: it reads files, calls `map-persist`, `map-draft` and `map-web`, and hands pages to `map-preview`.

- `shrsh draft preview <draft> [--listen <addr:port>]` watches the draft and its base. It rebuilds the page on each change, then reloads the browser.
- `shrsh draft check <draft>` prints the problems with file, line and column, and exits nonzero if there are any. Agents writing drafts can use this.
- `shrsh draft render <draft> --out <file.html>` writes a standalone page.

## 5. Changes to `map-web`

- **Drawing proposed parts.** Dash patterns already mean unreviewed and certain roles, so proposed parts need a different visual signal. A proposed module box is unfilled, with an outline in its own color and a "proposed" tag. A proposed arrow is drawn as a hollow double line in its role's color. Proposed routes follow the same rule. The legend gains an entry.
- **Filter.** A control shows observed parts, proposed parts, or both.
- **Panel.** It shows each part's status. It lists proposal notes under "Rationale", apart from source evidence. A map without a source says "Draft: no source analyzed" in place of the revision and build.
- **State in the address.** The selected module or route and the filter are kept in the URL fragment. A reload keeps the reader's place. This also makes links to a selection possible.
- The view types gain `status` on `Crate`, `Edge` and `Route`, and `kind` on `Evidence`. `Meta` gets an optional source.

## 6. Order of work

1. `project-map` type changes and `problems`. The example and `map-web` are updated to compile and treat every part as observed. One example route is marked proposed, so the next step has something to draw.
2. `map-web` drawing, filter, panel and URL state. This depends on step 1.
3. `map-persist` format, load and save. This depends on step 1 and can run in parallel with step 2.
4. `map-draft`. This depends on steps 1 and 3.
5. `map-preview`. It needs only the page as text, so it can start after step 1.
6. The `mapper` crate and `shrsh draft`. This depends on steps 2 to 5.
7. Convert sharsheret's own architecture map, now Rust code in `project_map::example`, into a draft file under `docs/architecture/`. Preview it with `shrsh draft preview`. The Rust example stays as a test fixture. Update `docs/architecture/decisions.md` to describe drafts.

## Tests

- `ProjectMap::problems`: one case per rule, plus a clean mixed map.
- `map-persist`: a saved map and a draft each survive a load and save unchanged. An unknown version, a missing required field, and a misspelled field are rejected with a line and column.
- `map-draft`: combining a draft with its base; an ID collision; a reference to a missing base part; a draft with no base.
- `map-web`: the view marks proposed modules, edges and routes and separates notes from evidence. A map without a source renders.
- `map-preview`: a change triggers a reload event. A failed load keeps the last good page and reports the problems.
- `shrsh draft check`: exit codes and the reported positions.

## Open questions for Sam

1. **Removing or changing observed parts.** A plan often removes a module, such as the readers in PR #6, or changes an observed module's concern. Two states cannot say "this observed part is proposed for removal." Should `Status` gain `ProposedRemoval`, and should a draft be able to propose a new description for an observed part? I recommend adding `ProposedRemoval` now and leaving changed descriptions for later.
2. **Overlay or full copy.** This plan makes a draft lay its changes over a base map file. The alternative is a full copy of the map with edits, which is simpler but goes stale when analysis reruns. Is the overlay right?
3. **Shorter route syntax.** Writing each hop's link by hand is tedious. `map-draft` could accept a route written as a list of modules and work out each link from the declared dependencies, rejecting ambiguous cases. Should that be in the first version or added later?
