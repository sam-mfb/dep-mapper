# Plan: graph-evaluator-manual

## Goal

`graph-evaluator-manual` lets an agent harness, such as Claude Code, produce a `GraphEvaluation`. The evaluator presents the graph to the agent. The agent judges modules and relationships and cites evidence. The evaluator checks each answer against the graph and returns the result through `GraphEvaluator::evaluate`. A person can use the same tools, so this replaces the terminal prompt described in `docs/architecture/decisions.md`.

`graph-evaluator-llm` differs: it calls a model API itself and owns the prompt and the conversation. Here the harness owns the conversation, and the evaluator supplies tools and checks.

## Constraints from the existing contracts

- `evaluate(&self, &ProjectGraph) -> Result<GraphEvaluation, EvaluationError>` is synchronous and takes the graph by reference.
- A `GraphRef` is opaque. Only the graph resolves it. Outside the process the agent needs a written form for each reference.
- A module the evaluation does not mention is unreviewed. A deferral needs a reason. A route can carry no relationship. How the evaluator records what it set aside is its own concern.
- The producer checks the snapshot and the references before `map-builder` checks them again.
- `project-graph` has no serialized form. The manual evaluator should define its own written form, not add one to `project-graph`.

## Options

### 1. How the graph is presented

| Option | Strengths | Weaknesses |
| --- | --- | --- |
| A. Files only. One file per module and per module pair, plus an index. | Any harness can read files. Nothing runs during the review. Easy to inspect and resume. | The agent writes answers by hand. Mistakes surface only at the end. |
| B. A CLI over a workbook directory. `show` prints a piece; `judge`, `relate` and `defer` record an answer. | Works in any harness that has a shell. Each answer is checked when it is recorded. The record stays on disk, so work survives a crash and can resume. | We build and document a small command set. |
| C. An MCP server with typed tools. | Typed inputs. Checks on every call. | The harness must be configured to connect. The evaluator must run a server for the whole review. It ties the evaluator to an MCP library, which we would have to wrap. |

### 2. How answers and citations are written

Each reference gets a written handle that names its kind and ID, such as `route:R12`, `fact:F3` or `item:I7`. Modules use `module:<id>`. The workbook alone maps handles to graph IDs, so `GraphRef` stays opaque inside the code.

Each answer is one record: a module decision (judged, with concern, kind, cluster, reason and citations; or deferred, with a reason), a relationship (kind, reason, citations), a cluster list, or a note that a route was considered and given no relationship. Recording the same module again replaces the earlier decision. Relationships carry IDs the evaluator assigns, so they can be revised or withdrawn.

### 3. How answers are checked

Two levels:

- **Rejected.** A handle that does not resolve. A module that is not in the graph. An unknown module kind or relationship kind. An empty reason. A judgment or relationship with no citations. A cluster that is not in the cluster list.
- **Warned, then kept.** A module judgment whose citations never touch that module. A relationship that cites no route. A kind that its evidence does not show, such as `Implements` without an `Implements` fact.

The CLI or server applies these checks as each answer arrives. `evaluate` then repeats every check against the in-memory graph, which is authoritative. If a rejected answer remains, `evaluate` returns an `EvaluationError` that lists each problem. It never drops an answer silently. Warnings travel with the workbook for the person reviewing the map.

### 4. How a large graph is split

The unit of work is a **piece**:

1. **Module piece.** One module with its items, build units, gaps, and the routes that start, pass through or end in it. Its outcome is a module decision.
2. **Cluster piece.** Every module decision so far. Its outcome is the ordered cluster list. It runs after the module pieces, and module judgments can then be moved between clusters.
3. **Pair piece.** The routes that join two modules, in either direction, and those that leave one module for external or unresolved endpoints. Its outcome is relationships. The module decisions for both ends are shown as context. A relationship that spans three modules, such as B adapting A for C, cites routes from more than one pair; the pair piece can show the other routes through the middle module on request.

A piece over a set size, such as 40 routes, is paged. The index lists every piece with its status: not started, in progress, done. Each piece keeps its answers in its own file, so several subagents can work on separate pieces at once without conflicting writes.

Source text: the graph holds spans as file and byte range, not text. The workbook shows each span as a path and line range. An injected `SourceText` dependency converts byte ranges to lines and can embed a short excerpt. The agent can read the rest from the checkout.

### 5. How the evaluator fits the synchronous contract

| Option | How it works | Strengths | Weaknesses |
| --- | --- | --- | --- |
| S1. Two runs. | The first `mapper` run writes the workbook and fails with "awaiting answers." The agent works. The second run reads the answers and returns. | `evaluate` never blocks. | The second run must reproduce the same graph ID. The first run ends in an error that is really a normal state. |
| S2. Block on an external agent. | `evaluate` writes the workbook, prints the instructions, and waits until the agent records `submit`. | One run. A person can start any harness session by hand. | `mapper` holds a process open for the whole review. Needs a timeout. |
| S3. Block on an agent the evaluator starts. | `evaluate` writes the workbook and calls an injected `AgentRunner`. One runner starts Claude Code non-interactively in the workbook; another is S2. | One run, fully automatic when wanted. Harness details stay behind our contract. | Depends on a harness CLI being installed, behind the runner. |

## Recommendation

Use a **workbook directory with a CLI (1B)**, the **written handles and two check levels above**, **module, cluster and pair pieces**, and **S3 with S2 as one of its runners**.

Reasons:

- A CLI gives the immediate checks of an MCP server without connecting or running one. Every harness with a shell can use it, and so can a person.
- Files make the review resumable. If a workbook for the same graph ID exists, `evaluate` keeps its answers and shows only unfinished pieces.
- Blocking inside `evaluate` keeps the graph in memory for the final check and keeps `mapper` to one run. Putting the harness behind `AgentRunner` keeps the evaluator independent of Claude Code.
- An MCP front end can be added later over the same workbook if typed tools prove worth it.

### Components

| Component | Concern | Effects |
| --- | --- | --- |
| Presentation | Turns a graph into pieces, handles and the index. | None. Source text comes from the injected `SourceText`. |
| Checking | Checks answers against the graph and builds the `GraphEvaluation`. Returns problems and warnings as values. | None. |
| Workbook | Reads and writes pieces and answer files in a directory. | File system, behind a `WorkbookStore` contract. |
| CLI | `index`, `show`, `judge`, `defer`, `relate`, `withdraw`, `clusters`, `skip-route`, `check`, `submit`. Checks each answer against the workbook's handle index. | Uses the workbook. |
| `ManualEvaluator` | Implements `GraphEvaluator`: presents, runs the agent, checks, returns. | Takes `WorkbookStore`, `SourceText`, `AgentRunner`, the page size and the timeout at construction. |

`AgentRunner` has one operation: given the workbook location and the instructions, return when the agent has finished or failed. The first implementations are a Claude Code runner and a wait-for-submit runner. Both live with `mapper`'s composition, not in the evaluator's core.

### Changes outside the crate

- `docs/architecture/decisions.md`: the manual evaluator presents the graph to an agent harness or a person through a workbook and CLI, not only a terminal prompt.
- The example architecture map gains `graph-evaluator-manual` → `AgentRunner` wiring when the crate lands.

### Tests to plan for

- Every rejection rule, and that `evaluate` reports, not drops, a rejected answer.
- A handle written by one graph's workbook does not resolve against another graph.
- Resuming a workbook keeps finished pieces.
- Paging and pair grouping on a graph with many routes.
- An end-to-end run with a scripted `AgentRunner` that records known answers through the CLI, against the kashkeshet backup example in `proof/`.

## Open questions for Sam

1. Should the cluster piece come before or after the pair pieces? Before is simpler. After would let relationships inform clusters.
2. Should warnings stay in the workbook only, or should `GraphEvaluation` gain a place for them? This plan keeps the contract unchanged.
3. Is a per-piece subagent split something the evaluator should arrange, or left to the harness? This plan leaves it to the harness and only makes it safe.
