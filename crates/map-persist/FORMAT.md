# Map and draft files, format 3

A file is TOML. It holds either a saved **map** or a **draft**. A map is a complete project map, such as one written after analysis. A draft holds proposed changes: new parts, new reviews for base parts, and base parts to remove. It is laid over a saved base map, or it stands alone as a map made only of proposals.

`map-persist` reads and writes this format. Values are checked when the file is loaded, and every error gives a line and column.

## Header

Every file starts with these two fields:

| Field | Value |
| --- | --- |
| `format` | `3`. Loading also reads formats 1 and 2, described under [Older formats](#older-formats), and refuses any other version. Saving always writes format 3. |
| `kind` | `"map"` or `"draft"`. |

## Maps

| Field | Required | Meaning |
| --- | --- | --- |
| `project` | yes | The project's name. |
| `source` | no | The analyzed build. Left out for a map with no analyzed source. |
| `layers` | yes | Layer names from top to bottom, each listed once. The first layer holds the program, which uses the others; the last holds the foundations. A module's layer normally sits at or above every layer it uses. |
| `modules` | no | Modules keyed by ID. Left out means none. |
| `dependencies` | no | An array of dependencies. Left out means none. |
| `routes` | no | Routes keyed by ID. Left out means none. |
| `evidence` | no | Evidence entries keyed by citation ID. Left out means none. |
| `gaps` | no | An array of evidence entries describing missing coverage. Left out means none. |

`source` has four required fields: `graph`, `revision`, `target` and `configuration`.

In a map, every module, dependency and route needs a `status`. A part whose status is `"proposed-update"` also needs a `proposed` review: the new review proposed for it. Its `review` stays the one found in code. No other part has a `proposed` review.

## Drafts

| Field | Required | Meaning |
| --- | --- | --- |
| `base` | see below | The path of the saved map the draft is laid over. |
| `project` | see below | The project's name, for a draft with no base. |
| `layers` | no | Layers to list after the base map's layers. Left out means none. |
| `modules`, `dependencies`, `routes`, `evidence` | no | As in a map. Left out means none. |
| `update` | no | New reviews proposed for base parts. Left out means nothing is updated. |
| `remove` | no | Base parts proposed for removal. Left out means nothing is removed. |

A draft has exactly one of `base` and `project`.

A draft's own `modules`, `dependencies` and `routes` are proposed additions. Their `status` may be left out; if given, it must be `"proposed-addition"`. Changes to base parts go under `update` and `remove`.

`update` has three fields, and each may be left out to mean none:

- `modules`: tables keyed by module ID.
- `dependencies`: an array of tables, each with `from` and `to`.
- `routes`: tables keyed by route ID.

An update gives only the fields it changes. The fields it leaves out keep the base part's values. A module's update may change `concern`, `kind` and `layer`. A dependency's or route's update may change `role` and `other_roles`. Each update changes at least one field, and needs its own `reason` and `primary` citation, usually a proposal note. `supporting` may be left out to mean none. If the base part has not been reviewed, the update must give every field a judgment needs. A dependency may be updated only once. A draft without a base must not update anything.

```toml
[update.modules.map-web]
kind = "logic"
reason = "Rendering becomes a pure function of the map."
primary = "note:cache"
```

`remove` has three fields, and each may be left out to mean none:

- `modules`: an array of module IDs.
- `dependencies`: an array of `{ from, to }` tables.
- `routes`: an array of route IDs.

A part may be listed only once. A draft without a base must not remove anything. Removing a module also marks every base dependency and route that touches it as proposed for removal, so they need not be listed.

## Parts

`status` is one of:

- `"observed"`: the part is in code.
- `"proposed-addition"`: the part is proposed and not in code.
- `"proposed-update"`: the part is in code, and a new review is proposed for it.
- `"proposed-removal"`: the part is in code and proposed for removal.

### Module

```toml
[modules.cache]
status = "proposed-addition"
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
used_by = ["map-preview", "map-web"]
```

- `name` and `review` are required. `surface` may be left out to mean none.
- A surface item needs `name` and `kind`, which is `type`, `value`, `function`, `method` or `module`. `signature` and `declaration` may be left out. A `declaration` is a location.
- A surface item's `used_by` lists the IDs of the other modules that use it, each once. Each must be a module on the map, or for a draft, on the map the draft makes with its base; the item's own module is refused. It may be left out to mean none, as a draft's items usually do. Loading sorts it. `map-builder` fills it from the graph's use facts, the same facts that draw dependency arrows.

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
use_sites = ["note:cache"]
origin = { module = "map-persist", item = "load" }
end = { state = "reached" }
review = { state = "unreviewed" }

[[routes.cached-load.hops]]
to = { module = "cache", link = "receiver-uses-sender" }
evidence = ["note:cache"]
support = "proposed"
```

- A route is one path a value takes. Uses at several sites can follow the same path, so a route lists them all.
- `origin`, `end` and `review` are required. `use_sites` is an array of citation IDs, one for each use the route explains, and may be left out to mean none. `hops` may be left out to mean none.
- A point has exactly one of these:
  - `module`, with an optional `item`;
  - `external`, naming a dependency outside the project;
  - `unresolved`, naming a point the analysis could not resolve.
- An origin has no `link`. A hop into a project module needs a `link`:
  - `receiver-uses-sender`: the module the value moves into uses the one it came from.
  - `sender-uses-receiver`: the module the value came from uses the one it moves into.
  - `same-module`: the value moves between items of one module.
  - `from-outside`: the value comes from outside the project.
- A hop needs `to`, `evidence` and `support`. `evidence` is an array of at least one citation ID: every fact behind the step, one for each use site that takes it, or a proposal note. `support` is `established`, `possible` or `proposed`. Only a `possible` hop may have a `condition`. `role` may be left out to mean none.
- `end` is `{ state = "reached" }`, or `{ state = "stopped", at = [<citation ID>, ...], reason = "..." }`. `at` has at least one citation ID: for each use that stopped, where it stopped.

### Review

A review's `state` is `unreviewed`, `deferred` or `judged`.

- An unreviewed review has no other fields.
- A deferred review needs `reason` and has no other fields.
- A judged review needs `reason` and `primary`, a citation ID. `supporting` is an array of citation IDs and may be left out to mean none.
  - For a module, it also needs `concern` and `kind` (`logic`, `effect`, `composition` or `contract`). `layer` may be left out.
  - For a dependency or route, it also needs `role`. `other_roles` is an array of roles and may be left out to mean none.

A role is one of `direct-use`, `implements`, `wires`, `supplies-input`, `full-injection`, `adapted-injection` or `incidental`. Any other role is written as `"other: <description>"`. The description is everything after `other:` and the one space that follows it.

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

## Older formats

Each version is checked against its own form. A file that uses a field or form from another version is refused.

### Format 2

Format 2 is format 3 without `used_by` on surface items. Format 3 added the field, and older srs builds refuse fields they do not know, so the version changed. Loading reads a format 2 file's surface items with no users. Such a map does not record its users, so the page says every item is unused by other modules until the map is built again. A format 2 file with `used_by` is refused.

### Format 1

Format 1 is format 2 with one citation where format 2 has a list. Loading reads it into the same map or draft, each citation becoming a list of one.

- A route has `use_site`, one citation ID that may be left out, in place of `use_sites`.
- A hop's `evidence` is one citation ID.
- A stopped end's `at` is one citation ID.

A format 1 file with a list, or a later file with a single ID or `use_site`, is refused.
