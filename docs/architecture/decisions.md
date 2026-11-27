# What sharsheret maps

Sharsheret (srs) helps a developer answer “What does this package expose, who has a way to use it, and how does its work reach them?” Its overview is a map of packages. A reader can follow an individual arrow down to the project code that explains it.

## Agreed scope

- Analyze the source of a project for an identified build. Preserve which packages, target and configuration the evidence describes.
- Follow relationships within the project. When a value enters a third-party dependency, record the visible handoff. If it reappears in project code, record the visible receiving site. Joining those sites requires evidence.
- Describe the ways a package is wired for use. Proving that an operation executes for some particular runtime input is outside the goal.

## Three responsibilities

| Responsibility | Decision |
| --- | --- |
| Establish evidence | Deterministic analysis records contract surfaces and arrows. It records what is exposed, where a name is imported or re-exported, where a type is mentioned, where a value is created, stored, passed or returned, and which method or operation is used. Keep the relevant method, value and source location behind each package-level relationship. |
| Interpret evidence | A human or LLM describes a module’s concern, assigns its kind, and labels particular routes or uses. Full injection, adaptation, incidental use, and mixed use are architectural judgments. Two routes between the same packages can receive different labels. |
| Present the interpretation | Draw the labeled modules and routes. Selecting a module reveals its concern, kind, provided surface, dependencies and consumers. The surface lists the module's exported type, value and function declarations as the grapher records them, without methods or submodules, which the graph still keeps. Each item names the other project modules that use it: those holding the subject of a use fact, such as an import, a reference or a call, whose object is the item. These are the facts that draw dependency arrows; route hops add nothing, and a use within the module does not count. It has known gaps. The Rust grapher records no item for a `macro_export` macro or an enum variant, and shows a `pub use` under the path where the item is defined. Associated constants declared in a trait or in a trait implementation stay, since the graph does not record an item's owner; the fix is for the Rust driver to record it. For the same reason, a use of a method does not mark its type as used. The TypeScript grapher records no item for `export default` of an expression, and an exported namespace is left out though its members stay. Each dependency and consumer shows how many uses it summarizes and opens to the one-hop routes behind them; the module's own route list holds the routes of more than one hop. Selecting a route reveals its direction, participating modules and supporting evidence, including alternatives or a gap. A label can be revised independently of the extracted facts. |

An unused import, use only in a type position, a public re-export and a call convey different things. Evidence for an unused import must apply to that import in the selected build. The reviewer can decide whether any of these observations indicate poor practice.

For example, kashkeshet calls the backup implementation itself and also passes it into install commands. Both uses belong on the map. In a different case, B might implement C’s interface and call A inside one method. To judge whether B adapts A for C, the reviewer needs to see which method C uses and what that method does with A. An adjacent call to a logger is evidence of a dependency, too.

Keep source-supported handoffs, possible connections, and the place a trace stops distinguishable. A missing route can mean incomplete evidence. The reviewer’s confidence in a label cannot supply a missing handoff.

The module boundaries, dependency arrangements and review workflow for sharsheret itself are proposals for design review.

## Map levels

| Level | Packages |
| --- | --- |
| Program | `mapper` |
| Mapping | `project-map`, `map-builder`, `map-web`, `map-persist`, `map-draft`, `map-preview` |
| Evaluation | `graph-evaluator`, `graph-evaluation-assembly`, `graph-evaluator-llm`, `graph-evaluator-manual` |
| Graphing | `project-graph`, `project-graph-rust`, `project-graph-rust-records`, `project-graph-typescript`, `route-grapher`, `route-cleanup` |

## Three core types

| Step | Type | Defined by | Produced by |
| --- | --- | --- | --- |
| Analyze the project | `ProjectGraph` | `project-graph` | The selected language implementation of `ProjectGrapher` builds the graph for the selected build and traces its routes. Routes group the facts; a direct use is a route of one hop. There is one route for each distinct path. Uses at several sites that follow the same path share a route, which names every site and cites every fact behind each hop. Each route retains its supporting evidence and gaps. |
| Evaluate the graph | `GraphEvaluation` | `graph-evaluator` | The selected evaluator obtains human or LLM judgments about the modules in the supplied graph, and relationships that interpret the graph's evidence. Each judgment cites the evidence it rests on. A subject may remain undecided. |
| Build the map | `ProjectMap` | `project-map` | `map-builder` combines the original `ProjectGraph` and its `GraphEvaluation`. It matches judgments to their subjects, preserves unreviewed routes and uncertain hops, and reports judgments whose subjects are missing. |

A `GraphEvaluation` names the particular graph snapshot it reviews. Its judgments use that snapshot’s identifiers. The graph records how modules are joined in the code; a relationship records what that means in the architecture. One route can carry several relationships, such as direct use and incidental use, and one relationship can draw on several routes, such as B adapting A for C. Two routes that take different paths can have the same endpoints and different relationships. Uses at different wiring sites that take the same path share one route, and the sites stay in it as evidence: a relationship that cites one site's fact still reaches the hop it supports. Each judgment and relationship includes its reason and the evidence it cites. The routes a relationship cites are the ones it interprets. A citation is a reference into the graph that does not reveal what it names; only the graph can resolve it. The evaluation therefore does not depend on how the graph groups its facts or on the kinds of evidence it holds. A deferred module records a reason, while one the evaluation does not mention remains unreviewed. A route can carry no relationship, and how an evaluator records what it set aside is its own concern. How routes are summarized into arrows between modules is decided when the map is built. `map-builder` draws an arrow from one module to another where a fact shows the first using the second, such as an import, reference or call, or where a route hop carries a value between them. A relationship reviews the routes it cites, the hops whose facts it cites, and the arrows whose facts or single-arrow routes it cites. `map-builder` returns the map together with a list of what it could not match.

A `ProjectGraph` records receiver bounds separately from witnessed handoffs. A route can cite known use sites and end at an unresolved handoff. A `ProjectMap` carries the resulting route, review status and source citations in its own representation so it can be opened independently. It also lists the dependencies between modules for the overview, keyed by the pair of modules each joins, with its evidence and review. A route explains a particular path, while a dependency summarizes every way one module uses another. Each hop into a project module says how it got there: the receiver uses the sender, the sender uses the receiver, it stayed in one module, or it came from outside the project. The first two name the dependency the hop travels along. The snapshot and reference checks belong to the producer and to `map-builder` when it joins the two inputs.

## Composition and effects

`mapper` owns the composition. It constructs the grapher for the project’s language and calls it through the `ProjectGrapher` contract in `project-graph` with the selected build. Each language crate builds the `ProjectGraph` end to end: it reads the source and traces its own routes. `project-graph-rust` serves the initial investigation, and `project-graph-typescript` implements the same contract for another language.

`project-graph-typescript` reads an npm workspace, one module per package, with the build selected by a tsconfig. It runs the TypeScript checker in a Node child process. The child uses the TypeScript 6 API through a small interface of our own, so the move to TypeScript 7 changes one file. It prints items, facts and receiver links as JSON Lines, and the Rust crate assembles the graph. It traces value routes with `route-grapher`, the shared tracer that `project-graph-rust` also uses.

## Layers

A map's layers run from top to bottom. The first layer holds the program, which uses the others; the last holds the foundations, which use none of them. A module's layer normally sits at or above every layer it uses. `Clusters` in `graph-evaluator` and `Layers` in `project-map` carry this order. The evaluators' checks warn, without refusing the answer, when a module uses a module in a layer listed above its own. A use here is a fact whose subject uses its object, the same rule `map-builder` follows to draw an arrow.

The layers can be fixed in advance. `srs map --layers` lists them, or `srs map --draft` takes them from a draft laid over its base map. The list is evaluator configuration, given at construction, and not part of the `GraphEvaluator` contract. With a fixed list, the evaluator does not propose layers, every module judgment must name one of them, and a module that fits none is deferred with a reason. Only the list is fixed. A draft also supplies each module's earlier layer, which module pieces show as a hint. Without a fixed list, as on a project's first run, the evaluator proposes the layers in this order. A draft can add layers below its base's but cannot reorder them, so the base map's order, set by the evaluator that produced it, stays in force.

## Excluded crates

Some outside crates are, in effect, part of the language. Every module uses them, so their routes say little about the architecture and crowd out the ones that do. Each grapher takes a list of outside crates or packages at construction and leaves out the routes that start in them. The list is grapher configuration. It is not part of the `ProjectGrapher` contract or the `BuildSelection`.

- A route is left out when its origin is an outside endpoint in a listed crate. The grapher decides which names lie in a crate. For Rust, it is the first segment of the name: the dependency the project declares, or the toolchain crate. So `serde` also covers `serde_core`, reached through `serde`. For TypeScript, it is the package before the `#`.
- A fact is left out when only those routes cite it. A fact between project modules stays, even when its route passed through a listed crate. So does a fact another remaining fact cites.
- Nothing is left out silently. The graph gets a coverage gap that names the list and says how many routes and facts were removed, every module piece of the evaluator's workbook shows it, so human and LLM evaluators judge knowing what is missing, and the map page shows it with the other coverage gaps. The Rust graph's ID ends with the list, and the TypeScript graph's ID hashes its contents, so graphs made with different lists have different IDs.
- `srs map` passes a default list, written out in `mapper`. For Rust it is `std`, `core` and `alloc`, plus the crates that shape the language in rule 7 of our Rust principles: `thiserror`, `futures`, `serde`, `serde_json`, `tokio` and `tokio-util`. For TypeScript it is empty. `--exclude` replaces it, and an empty `--exclude` keeps every route.

The shared library `route-cleanup` does the leaving out and the recording. Each language crate calls it with its own way of matching names.

## Merged routes

Without merging, each use site yields its own route, so one call made from many places becomes many near-identical routes. Each grapher therefore merges routes that follow the same path before it leaves out excluded ones. Two routes follow the same path when they have the same origin, the same hops in order, each to the same endpoint with the same support, and the same end: both reach their use, or both stop for the same reason. The merged route names every use site, each hop cites every fact behind it, and a stopped end cites every place its uses stopped. Facts stay one per site; nothing is dropped. A path that differs in any step stays a separate route. As a refinement, a route that never leaves one module (every hop lands in that module, its origin lies outside the project or in that module, and it reaches its use) ends at the module rather than at items inside it, so uses of one outside item anywhere in a module become one route; routes that cross modules or stop keep their detail. The same library, `route-cleanup`, does the merging for both graphers. The exclusion counts that follow are of merged routes.

Analysis is not split into a language reader and a shared grapher. If some route logic turns out to be the same across languages, it goes in a library the language crates call, not in a separate stage of the pipeline.

`mapper` passes the resulting `ProjectGraph` to the selected `graph-evaluator`. The manual implementation writes the graph into a workbook of pieces and hands it, through an injected runner, to an agent harness such as Claude Code or to a person. They read the pieces and record answers with `srs eval` commands, which check each answer and its citations as it is recorded. The evaluator checks every answer again before it returns the evaluation. The LLM implementation calls the Claude API itself, one piece of the graph at a time. It reaches the model through its own `ModelClient` contract and sends HTTP through an injected `HttpTransport`, so the provider and the HTTP library stay behind contracts this project defines. Both return a `GraphEvaluation` through the same contract. Both cut the graph into the same pieces, cite evidence by the same written handles, and check answers by the same rules, all from `graph-evaluation-assembly`.

`mapper` retains the original graph and calls `map-builder` with both values. The transformation `ProjectGraph + GraphEvaluation → ProjectMap` is pure. `mapper` sends the returned map to `map-persist` for storage and `map-web` for display. A saved map can be reopened without rerunning analysis or evaluation.

## Drafts: planning with the map

A map can also describe a plan. Each module, dependency and route in a `ProjectMap` has a status. It is observed in code, or it is a proposed addition, update or removal. An update keeps the review found in code and carries the proposed review beside it. A part in code cannot rest on an addition or cite a proposal note. A proposal can rest on parts in code, but not on one proposed for removal. `ProjectMap::problems` checks these rules.

A draft is a file of proposals. It is laid over a saved base map, or it stands alone as a map made only of proposals. It lists new parts, partial updates of base parts and base parts to remove. An update gives only the fields it changes; the rest come from the base. Removing a module also removes the dependencies and routes in code that touch it. When analysis runs again and the base map is saved again, the draft still applies to it. `map-persist` reads and writes drafts and saved maps in one versioned TOML format. `map-draft` lays a draft over its base and reports what does not fit. Drafts are edited only as files; there is no editing in the browser.

`map-web` draws proposals distinctly and offers three views. Current shows what is in code. Planned shows the code after the proposals. Changes shows both, with every proposal marked. The page keeps the selected module, route and view in its address.

`srs draft check` reports a draft's problems with file, line and column. `srs draft render` writes its page. `srs draft preview` serves the page through `map-preview` and reloads the browser when the draft or its base changes. A draft with problems leaves the last good page shown, with the problems listed above it.

sharsheret's own architecture map is `docs/architecture/sharsheret-map.toml`, which `srs map` produces from sharsheret's code; `crates/mapper/README.md` gives the command to refresh it. Planned changes go in the draft `docs/architecture/sharsheret.toml`, laid over that map. The draft holds only what should differ from the code, so it stays small and still applies after the map is refreshed. Today it proposes nothing, because the analyzed map already states each module's concern by its own responsibility. Check it with `srs draft check` and preview it with `srs draft preview`. The Rust map in `project_map::example` is a fixture for tests.

On the architecture map, a solid gray “used by” arrow covers use of a type, value or operation. The dashed wiring arrow shows where `mapper` binds the selected implementation. “Supplies input to” identifies the caller and recipient.
