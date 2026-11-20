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

## Review decisions and composition

`graph-evaluator` is a contract for making an architectural judgment. The request takes the shared `DependencyGraph` value and identifies the module or route to review. The reviewer reads the supporting evidence from that graph. Its response ties the judgment to that subject and evidence. A reviewer can leave a subject undecided.

Two proposed effect modules implement the contract. `graph-evaluator-manual` displays the evidence in the terminal, asks a human to decide, and returns the answer. `graph-evaluator-llm` submits the review request to a model API and returns the model’s decision. A different review mechanism can implement the same contract.

`mapper` owns the composition. It selects a reader for the project language and supplies it to `project-grapher` through `project-reader`. `project-reader-rust` serves the initial investigation. `project-reader-typescript` illustrates how another language can supply the same observations. `mapper` binds either the LLM evaluator or the manual evaluator to `graph-evaluator`. The dashed arrow from the command to that contract shows where the selected implementation is wired. `mapper` passes the resulting graph and review subject through the contract. It passes the resulting DependencyGraph and ArchitectureJudgments to `map-builder`.

## Building and consuming ProjectMap

| Type | Owner | Meaning |
| --- | --- | --- |
| `ProjectObservations` | `project-reader` | Build-scoped facts returned by the selected reader. |
| `DependencyGraph` | `dependency-evidence` | Modules, dependencies and routes derived from observations, with their evidence and any gaps. |
| `ArchitectureJudgments` | `graph-evaluator` | Decisions about identified modules and routes, made by the selected reviewer. |
| `ProjectMap` | `project-map` | A self-contained map associating the graph’s modules and routes with judgments and supporting evidence. |

`dependency-evidence` is a proposed shared data-contract module. `project-grapher` produces its `DependencyGraph` value. `mapper` supplies the same graph to `graph-evaluator` for judgment and to `map-builder` for construction. `mapper` composes `project-grapher` and `map-builder`.

`project-map` is the contract package that defines the shared `ProjectMap` type. `map-builder`, `map-persist` and `map-web` all depend on this contract.

`map-builder` is a pure logic module. Its contract is `DependencyGraph + ArchitectureJudgments → ProjectMap`. It matches judgments to their subjects, retains unreviewed routes and uncertain hops, and reports judgments whose subjects are missing. Repeating the transformation on the same inputs produces the same map.

`mapper` directly depends on both logic modules. It calls `project-grapher` to build the graph, obtains the judgments, then calls `map-builder` and supplies the result to the consumers. `map-persist` saves and loads a ProjectMap. `map-web` provides the web surface for `mapper`. It presents a `ProjectMap` and owns its visual layout and interactions. A saved map can be reopened without rerunning analysis or review.
