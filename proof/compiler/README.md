# What a Rust dependency mapper can establish

The input is a Cargo build selection. The collector has no supplier, consumer, expected route, crate-name rules or architecture file. It records compiler facts across the selected build. The TypeScript analysis starts at calls and searches backward for the supplied values. A separate query selects a crate after analysis. The same crate can appear as an importer, implementer, caller and injector.

## What each result means

| Question | Evidence and boundary |
| --- | --- |
| Which build units require which others? | Cargo's unit graph supplies package identities, target kinds, features, dependency names and edges for this invocation, including build dependencies. Another platform, feature selection or test target is a different inventory. A build edge alone says nothing about a particular operation. |
| Who names, imports or publicly imports an item? | In expanded Rust code the collector visits HIR paths with external definition IDs and distinguishes use items by their declared visibility. This covers type and constant paths as well as functions. A public use inside a private module can still have limited visibility. Implicit operations, macro invocations and effective public API reachability need their own inventories. |
| Who implements a contract? | Expanded local trait impl declarations identify the contract and self type. Explicitly defined members point to the corresponding trait members. Generic and blanket impls are declarations with conditions. |
| Who calls what? | Every call and tail-call terminator in each captured, pre-optimization MIR body gets a basic block, source span, actual operands and dispatch category. A named call identifies a definition. For eligible concrete trait calls rustc selects the implementation or confirms virtual dispatch. Other trait calls retain the contract, and indirect calls retain the call site. A foreign declaration identifies a boundary. Implicit drops and inline assembly are outside this call inventory. |
| Whose implementation arrives at the consumer? | Trace the receiver backward through control-flow predecessors, local definitions, shared borrows, fields, tuples, captures, argument passing and returned values. Keep a call stack so a return goes back to its particular caller. A concrete-to-trait-object conversion identifies a supplied type. Several conversions can reach one invocation. The result is a structural path, conditional on the flow model. |
| Whose work does an adapter deliver? | Having traced the supplied implementation, match its method to the invoked contract member. Report external calls made by that method separately from external call results that flow to its return. This distinguishes a logger in the method from a supplier whose value the method returns. Effects, arbitrary result transformations and architectural intent need more evidence. A complex or ambiguous impl can have a known nominal provider and an unresolved method body. |

Definition IDs are scoped to a build. The presentation includes readable names, source spans and crate identities. Joins use identities, including when source spells a dependency through an alias or public re-export.

## Measured on kashkeshet

The pin for this experiment is [next at 41fbc289](https://github.com/sam-mfb/kashkeshet/commit/41fbc289dbfe68be90840d107e217762229b9745). The older proofs in the parent directory retain their original pin. This run selected the kkt binary and kashkeshet library on aarch64 Linux, default features, dev profile and workspace-body collection.

| Observation | Count |
| --- | ---: |
| Cargo units / dependency edges | 323 / 901 |
| Instrumented crates / captured bodies | 67 / 7,909 |
| Compiler-listed MIR keys / listed keys without a collected body | 8,382 / 473 |
| Call and tail-call sites in captured bodies | 46,490 |
| Named direct calls / compiler-selected static trait calls | 24,418 / 15,883 |
| Trait calls left at the contract / compiler-confirmed virtual calls | 4,832 / 1,349 |
| Indirect / foreign calls | 6 / 2 |
| Virtual sites with at least one provider witness / without one | 297 / 1,052 |
| Virtual sites with multiple provider types found | 10 |
| Virtual sites whose contract belongs to another indexed crate / with a provider witness | 300 / 55 |

The 473 missing body records are all constructor functions listed by rustc, 412 for variants and 61 for structs. Every other listed key had a captured body. Of the 297 sites with a structural witness, 66 also had an unresolved incoming path. The cross-crate row uses the location of the contract declaration. An adapter can cross crate boundaries on its way to a consumer that declares its own contract.

For written calls to kkt contracts, the matched-site comparison is 173 of 668 before and 293 of 668 now. The follow-up accounts for all 495 original misses and tests which providers the build and runtime conditions select. [Coverage and condition investigation](investigation.md)

Two supplier-only queries recover the motivating distinctions.

* **kkt-backup-os.** The program calls write and prune on OsBackups itself. Separately it fills the backups field, and install commands invoke Backups::write in local_backup. This yields both a direct use and a full injection. [Program calls](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/commands/serve.rs#L296) · [Supply site](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/native.rs#L100)
* **kkt-llm-agent-command.** The method bodies in kashkeshet's ClaudeCommand call the supplier's add and remove operations. The program puts that adapter in agent parts. The trace proceeds through kkt-agent-commands to the contract invocations inside a kkt-llm-agent-config closure. The returned error goes through the standard library's map_err. This build indexes the call to map_err and does not trace through its body. The result therefore reports an injected method that calls the supplier. [Adapter methods](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/client_config.rs#L58) · [Consumer calls](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-llm-agent-config/src/files.rs#L179)

## What the independent fixture proves

One compiled consumer reads a field. A runtime argument selects either the supplier's Factory implementation or the caller's Bridge implementation. Bridge calls an audit function, calls a helper, and returns a measurement from the supplier through a renamed Cargo dependency and a re-export. The analysis finds both providers, identifies the measurement as a return source, and excludes the adjacent spare field and another same-trait implementation. Running the same binary twice produces 11 and 22.

A second consumer is called only when that same flag is false. Its possible runtime provider is always Factory. The analysis now follows the immutable flag across the helper call and rules out Bridge on that path. A third consumer receives a slot edited through a mutable reference. The mapper stops at the borrow and reports an unknown. The executable checks that the new provider really was installed. The tests check both the corrected false positive and the remaining unknown.

## What complete could mean

Cargo's unit graph is enumerable for a fixed successful build. Source relationships also become checkable once their units are defined, for example one edge for every external resolved HIR path or one observation for every MIR call terminator. A check can reconcile the compiler's list of bodies with the captured bodies, then reconcile the call inventory with its categories. For each confirmed virtual site this experiment records a provider witness, a stopping reason, evidence of an excluded path, or a combination. Its missing-body list and the unselected trait calls remain part of the result.

Exact feasible work paths for arbitrary programs have a different limit. Put a call to the supplier after an arbitrary computation. Deciding whether execution can reach that call in every such program would decide whether that computation halts. Even in a terminating program, a command-line choice can select a different provider each run. A plugin discovered at runtime may supply an implementation outside the build entirely. Runtime observation can say which path a particular run took.

Many of the current misses are engineering limits. The prototype has a bounded backward search, recognizes selected projection and pointer shapes, joins explicit methods for simple nominal implementations, and follows indexed calls. It omits many container operations, enum flows, mutation, uninstantiated generic calls, callbacks and code beyond the inventory. It does not establish alias safety or branch feasibility for arbitrary unsafe Rust. The stopping reasons are local to the explored search. A site with one witness may still have other origins.

For a mapper, publish declared, referenced, implemented, directly called, supplied, forwarded and invoked as separate facts. Mark alternatives and unresolved hops at the precise location. Adding compiler support for a class of operations can then be evaluated against the same denominator. Naming an abstraction's concern or explaining why a side effect is its primary work remains editorial. The compiler can supply evidence for that judgment.

## Reproduce

Requires Linux, rustup with Rust 1.98.1 and rustc-dev, Node 22.23 or compatible native TypeScript support, and the subject's build prerequisites and Cargo.lock. Python 3 runs the assertions. The collector uses a pinned private rustc API. The unit graph uses Cargo's matching unstable schema. A run requires a fresh output directory. By default it compiles the workspace and indexes workspace crates. Pass Cargo selection flags to choose another build, or use --include-dependencies before the workspace path to instrument compiled third-party crates as well. Prebuilt sysroot crates remain external.

```sh
rustup component add rustc-dev --toolchain 1.98.1
python3 proof/compiler/test.py
CARGO_PROFILE_DEV_DEBUG=0 proof/compiler/run /path/to/pinned/kashkeshet /tmp/kkt-report -p kashkeshet --bin kkt --lib
python3 proof/compiler/test_kkt.py /tmp/kkt-report
node proof/compiler/query.ts /tmp/kkt-report kkt-backup-os
node proof/compiler/query.ts /tmp/kkt-report kkt-llm-agent-command
```

The report directory contains the build selection, compiler version, Cargo unit graph, per-crate facts, body coverage, routes and summary counts. Each supplier query prints JSON Lines. The fixture also checks feature changes, a supplier outside the workspace, repeat analysis and an independent rebuild.
