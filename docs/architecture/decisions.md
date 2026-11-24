# What dep-mapper maps

Dep-mapper helps a developer answer “What does this package expose, who has a way to use it, and how does its work reach them?” Its overview is a map of packages. A reader can follow an individual arrow down to the project code that explains it.

## Agreed scope

- Analyze the source of a project for an identified build. Preserve which packages, target and configuration the evidence describes.
- Follow relationships within the project. When a value enters a third-party dependency, record the visible handoff. If it reappears in project code, record the visible receiving site. Joining those sites requires evidence.
- Describe the ways a package is wired for use. Proving that an operation executes for some particular runtime input is outside the goal.

## Three responsibilities

| Responsibility | Decision |
| --- | --- |
| Establish evidence | Deterministic analysis records contract surfaces and arrows. It records what is exposed, where a name is imported or re-exported, where a type is mentioned, where a value is created, stored, passed or returned, and which method or operation is used. Keep the relevant method, value and source location behind each package-level relationship. |
| Interpret evidence | A human or LLM describes a module’s concern, assigns its kind, and labels particular routes or uses. Full injection, adaptation, incidental use, and mixed use are architectural judgments. Two routes between the same packages can receive different labels. |
| Present the interpretation | Draw the labeled modules and routes. Selecting a module reveals its concern, kind, provided surface, dependencies and consumers. Selecting a route reveals its direction, participating modules and supporting evidence, including alternatives or a gap. A label can be revised independently of the extracted facts. |

An unused import, use only in a type position, a public re-export and a call convey different things. Evidence for an unused import must apply to that import in the selected build. The reviewer can decide whether any of these observations indicate poor practice.

For example, kashkeshet calls the backup implementation itself and also passes it into install commands. Both uses belong on the map. In a different case, B might implement C’s interface and call A inside one method. To judge whether B adapts A for C, the reviewer needs to see which method C uses and what that method does with A. An adjacent call to a logger is evidence of a dependency, too.

Keep source-supported handoffs, possible connections, and the place a trace stops distinguishable. A missing route can mean incomplete evidence. The reviewer’s confidence in a label cannot supply a missing handoff.

The module boundaries, dependency arrangements and review workflow for dep-mapper itself are proposals for design review.

## Map levels

| Level | Packages |
| --- | --- |
| Program | `mapper` |
| Map | `project-map`, `map-builder`, `map-web`, `map-persist` |
| Evaluation | `graph-evaluator`, `graph-evaluator-llm`, `graph-evaluator-manual` |
| Evidence | `project-graph`, `project-grapher`, `project-reader`, `project-reader-rust`, `project-reader-typescript` |

## Three core types

| Step | Type | Defined by | Produced by |
| --- | --- | --- | --- |
| Analyze the project | `ProjectGraph` | `project-graph` | `project-grapher` builds the graph from the selected reader’s observations. Each route retains its supporting evidence and gaps. |
| Evaluate the graph | `GraphEvaluation` | `graph-evaluator` | The selected evaluator obtains human or LLM judgments about modules and routes in the supplied graph. Each judgment cites its subject and supporting evidence. A subject may remain undecided. |
| Build the map | `ProjectMap` | `project-map` | `map-builder` combines the original `ProjectGraph` and its `GraphEvaluation`. It matches judgments to their subjects, preserves unreviewed routes and uncertain hops, and reports judgments whose subjects are missing. |

A `GraphEvaluation` names the particular graph snapshot it reviews. Its module and route judgments use that snapshot’s identifiers. Two paths through different wiring sites can have the same endpoints and different judgments. Each judgment includes its reason and supporting citations. A deferred subject records a reason, while a subject omitted from the evaluation remains unreviewed.

A `ProjectGraph` records receiver bounds separately from witnessed handoffs. A route can cite a known use site and end at an unresolved handoff. A `ProjectMap` carries the resulting route, review status and source citations in its own representation so it can be opened independently. It also lists the dependencies between modules for the overview, keyed by the pair of modules each joins, with its evidence and review. A route explains a particular path, while a dependency summarizes every way one module uses another. Each hop into a project module says how it got there: the receiver uses the sender, the sender uses the receiver, it stayed in one module, or it came from outside the project. The first two name the dependency the hop travels along. The snapshot and reference checks belong to the producer and to `map-builder` when it joins the two inputs.

## Composition and effects

`mapper` owns the composition. It supplies the chosen `project-reader` implementation to `project-grapher`. The language reader returns build-scoped `ProjectObservations` for analysis. `project-reader-rust` serves the initial investigation, and `project-reader-typescript` illustrates the same contract for another language.

`mapper` passes the resulting `ProjectGraph` to the selected `graph-evaluator`. The manual implementation presents evidence in the terminal and asks a human to decide. The LLM implementation submits the evidence to a model API. Both return a `GraphEvaluation` through the same contract.

`mapper` retains the original graph and calls `map-builder` with both values. The transformation `ProjectGraph + GraphEvaluation → ProjectMap` is pure. `mapper` sends the returned map to `map-persist` for storage and `map-web` for display. A saved map can be reopened without rerunning analysis or evaluation.

On the architecture map, a solid gray “used by” arrow covers use of a type, value or operation. The dashed wiring arrow shows where `mapper` binds the selected implementation. “Supplies input to” identifies the caller and recipient.
