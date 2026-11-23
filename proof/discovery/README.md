# Discover an adapter path from its supplier

From the dep-mapper checkout, run:

```sh
python3 proof/discovery/run.py /path/to/pinned/kashkeshet kkt-llm-agent-command
```

The first argument selects the checkout. The only relationship input is the supplier package. The program compiles the default Linux kkt executable and its workspace libraries at 4c520c70618845c90fd62c0e3c6a9961cbf3dbff. It requires Python 3, the pinned Rust 1.98.1 toolchain, Cargo dependencies and a Linux build toolchain.

Each PATH is one source-backed example of how a supplier operation can reach a consumer. For the command package, two paths lead from Claude’s add_server and remove_server, through the program’s ClaudeCommand adapter, the client_command field and connect, to calls by the configuration library. Source spans identify the handoffs. The installation parts also receive this adapter. The prototype reports that starting point as unresolved.

## How it finds a route

1. Read compiled function bodies for the selected build. Find calls to operations in the supplied crate. Use rustdoc’s resolved impl and trait identities to identify which of the enclosing methods implement another crate’s contract. Candidate importers are found before requesting rustdoc data.
2. Find sites that convert the concrete adapter into a trait object. Start a value-flow search at each of those expressions. Follow local moves and borrows, aggregate fields by index, direct function arguments and returns, and calls on captured closures. Keep the field index when passing whole structures, and match returns to the calling site when the search entered a callee.
3. Report a consumer when the tracked object becomes the receiver of the matching contract method. Keep the source span of the supplier call, each handoff and the final invocation. An implementation declaration by itself never produces a PATH.

The result is a static *possible* use. Conditional calls can have paths even on inputs that skip them. The search is bounded, uses single-definition locals, and keeps one witness for each reached search state. It does not recover every use or execution path. It does not solve mutable aliasing, branch feasibility, arbitrary function pointers, indirect or generic dispatch, suppliers called only via helpers, or every macro, coercion and compiler representation. An unrecognised operation can stop a path. The reported search-cutoff count covers explicit limits and ambiguous local definitions, not all unsupported constructs. An empty result is inconclusive.

## Checks

```sh
python3 proof/discovery/test.py /path/to/pinned/kashkeshet
```

The test creates a disposable checkout and runs the same extractor on each build. Only the test assertions and fixture edits know expected consumers, adapters or method names. It checks the two command paths, repeats the query for identical output, asks independently about kkt-terminal, renames the command adapter and rebuilds, then supplies another implementation that does no work for the command crate. The last change leaves the original provider calls and adapter implementation in the build. The connect route must disappear.
