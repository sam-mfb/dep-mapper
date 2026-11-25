# Map and draft files, format 1

A file is TOML. It holds either a saved **map** or a **draft**. A map is a complete project map, such as one written after analysis. A draft holds proposed changes. It is laid over a saved base map, or it stands alone as a map made only of proposals.

`map-persist` reads and writes this format. Values are checked when the file is loaded, and every error gives a line and column.

## Header

Every file starts with these two fields:

| Field | Value |
| --- | --- |
| `format` | `1`. Loading refuses any other version. |
| `kind` | `"map"` or `"draft"`. |

## Maps

| Field | Required | Meaning |
| --- | --- | --- |
| `project` | yes | The project's name. |
| `source` | no | The analyzed build. Left out for a map with no analyzed source. |
| `layers` | yes | Layer names from top to bottom, each listed once. |
| `modules` | no | Modules keyed by ID. Left out means none. |
| `dependencies` | no | An array of dependencies. Left out means none. |
| `routes` | no | Routes keyed by ID. Left out means none. |
| `evidence` | no | Evidence entries keyed by citation ID. Left out means none. |
| `gaps` | no | An array of evidence entries describing missing coverage. Left out means none. |

`source` has four required fields: `graph`, `revision`, `target` and `configuration`.

In a map, every module, dependency and route needs a `status`.

## Drafts

| Field | Required | Meaning |
| --- | --- | --- |
| `base` | see below | The path of the saved map the draft is laid over. |
| `project` | see below | The project's name, for a draft with no base. |
| `layers` | no | Layers to list after the base map's layers. Left out means none. |
| `modules`, `dependencies`, `routes`, `evidence` | no | As in a map. Left out means none. |
| `remove` | no | Base parts proposed for removal. Left out means nothing is removed. |

A draft has exactly one of `base` and `project`.

In a draft, a part's `status` may be left out. It then means `"proposed"`.

`remove` has three fields, and each may be left out to mean none:

- `modules`: an array of module IDs.
- `dependencies`: an array of `{ from, to }` tables.
- `routes`: an array of route IDs.

A part may be listed only once. A draft without a base must not remove anything. Removing a module also marks every base dependency and route that touches it as proposed for removal, so they need not be listed.

## Parts

`status` is `"observed"`, `"proposed"` or `"proposed-removal"`. A part proposed for removal still exists in code.

### Module

```toml
[modules.cache]
status = "proposed"
name = "cache"

[modules.cache.review]
state = "judged"
concern = "Keeps recent maps."
kind = "effect"
layer = "Map"
reason = "Loading is slow."
primary = "note:cache"

[[modules.cache.surface]]
name = "Cache"
kind = "type"
```

- `name` and `review` are required. `surface` may be left out to mean none.
- A surface item needs `name` and `kind`, which is `type`, `value`, `function`, `method` or `module`. `signature` and `declaration` may be left out. A `declaration` is a location.

### Dependency

```toml
[[dependencies]]
from = "cache"
to = "map-persist"
evidence = ["note:cache"]
review = { state = "judged", role = "direct-use", reason = "Reads through it.", primary = "note:cache" }
```

- `from` uses `to`. A pair may appear only once.
- `review` is required. `evidence` may be left out to mean none.

### Route

```toml
[routes.cached-load]
origin = { module = "map-persist", item = "load" }
end = { state = "reached" }
review = { state = "unreviewed" }

[[routes.cached-load.hops]]
to = { module = "cache", link = "receiver-uses-sender" }
evidence = "note:cache"
support = "proposed"
```

- `origin`, `end` and `review` are required. `use_site` is a citation ID and may be left out. `hops` may be left out to mean none.
- A point has exactly one of these:
  - `module`, with an optional `item`;
  - `external`, naming a dependency outside the project;
  - `unresolved`, naming a point the analysis could not resolve.
- An origin has no `link`. A hop into a project module needs a `link`:
  - `receiver-uses-sender`: the module the value moves into uses the one it came from.
  - `sender-uses-receiver`: the module the value came from uses the one it moves into.
  - `same-module`: the value moves between items of one module.
  - `from-outside`: the value comes from outside the project.
- A hop needs `to`, `evidence` (a citation ID) and `support`. `support` is `established`, `possible` or `proposed`. Only a `possible` hop may have a `condition`. `role` may be left out to mean none.
- `end` is `{ state = "reached" }`, or `{ state = "stopped", at = <citation ID>, reason = "..." }`.

### Review

A review's `state` is `unreviewed`, `deferred` or `judged`.

- An unreviewed review has no other fields.
- A deferred review needs `reason` and has no other fields.
- A judged review needs `reason` and `primary`, a citation ID. `supporting` is an array of citation IDs and may be left out to mean none.
  - For a module, it also needs `concern` and `kind` (`logic`, `effect`, `composition` or `contract`). `layer` may be left out.
  - For a dependency or route, it also needs `role`. `other_roles` is an array of roles and may be left out to mean none.

A role is one of `direct-use`, `implements`, `wires`, `supplies-input`, `full-injection`, `adapted-injection` or `incidental`. Any other role is written as `"other: <description>"`.

## Evidence

```toml
[evidence."note:cache"]
kind = "proposal"
summary = "Previews are slow to load."
```

- `kind` is `source` (found by analyzing the source) or `proposal` (an author's reasoning). Both are required, together with `summary`.
- `unit` (a build unit) and `location` may be left out.
- A location is `{ file, start, end }`: a project-relative path and a half-open range of byte offsets, with `start` no greater than `end`.

## Saving

Saving writes every `status`, even in a draft, and writes empty lists where the value is empty. Loading the saved text gives back the same map or draft.
