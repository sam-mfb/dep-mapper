# How far a Rust dependency map can go

On the pinned Linux kkt executable we could identify the crate that supplies the receiver at every one of 668 virtual calls to workspace contracts, conditional on the call executing. The automatic analysis found a context-sensitive handoff chain at 391 sites. A provider identity alone does not explain who constructed it, who passed it on, what its method does, or whether execution reaches that method.

## What we measured

The revision is 41fbc289dbfe68be90840d107e217762229b9745. The target is the aarch64 Linux kkt executable with its library, default features, dev profile, rustc 1.98.1. There are 1,349 virtual call sites in the captured workspace bodies. The remaining 681 call contracts defined in core (663) or toml_edit (18). Cargo dependencies, static calls, generic trait calls awaiting selection, indirect calls, macro expansion and foreign calls have separate inventories. The denominator here counts one source MIR call site, regardless of how many times it executes or how many generic instances contain it.

| Evidence at the same call sites | Previously traced | Previously missing | Total |
| --- | ---: | ---: | ---: |
| Source handoffs found while matching calls and returns | 293 | 98 | 391 |
| Candidate handoffs found by allowing joins across callers and stored values | 0 | 263 | 263 |
| No connected chain recovered by either analysis | 0 | 14 | 14 |
| Total | 293 | 375 | 668 |

The compiler pass asks the selected executable for the implementations of each workspace trait across its dependency graph. It records separate evidence from generated dispatch tables and from concrete generic instantiations. It uses the executable's crate identities to scope those observations. If only one implementation can supply a particular method body, the contextual tracer can cross that call and continue tracing its arguments and return. A second search matches field, enum and async-result projections through the captured program. It computes reachability to exhaustion in its finite graph. It merges instances of a workspace field and calls to the same function. Consequently an allowed chain can join two different objects or invocations. A missing transfer in either model can also hide a real chain. Neither count estimates precision or runtime frequency.

For the motivating adapter case, starting from kkt-llm-agent-command finds kashkeshet's ClaudeCommand and the two ClientCommand operations invoked inside kkt-llm-agent-config. For both add_server and remove_server, the contextual search finds two distinct supply sites for that same adapter. One path runs through kkt-agent-commands. The second runs through kkt-install-commands and kkt-setup. A single edge between the supplier and consumer would erase an actual wiring difference. [Agent wiring](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/agent_parts.rs#L38) · [Install wiring](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/native.rs#L87)

### Which crate supplies the receiver

* At **643** sites, every eligible concrete receiver type comes from one crate. At 600 of those sites there is one impl declaration. At 43 sites the crate defines multiple eligible types. This conclusion follows from the compiler's declarations given a valid Rust trait object in this closed executable.
* At **23** service-manager sites, three crates declare implementations. The workspace has three coercions to this contract, all in one factory. The compiled target conditions permit only the systemd branch on Linux. The observed dispatch table independently names Systemd. The same reasoning must be repeated for a different build. [Service factory](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/service.rs#L10)
* At **two** browser-login calls, both TerminalLink and NoLogin appear in the executable. Only provisioning calls the helper containing these two operations. The install command supplies its login field to provisioning, which passes it to the helper. The process constructs that field from TerminalLink. The two NoLogin constructions enter add-admin-key and upgrade, neither of which calls the login helper. The contextual tracer finds TerminalLink at both calls. Replacing the provisioning argument with NoLogin in a disposable checkout changes both contextual results, while both eligible types and both tables stay the same. The broader field search lists both providers before the change. This last exclusion uses a source audit specific to kkt. [Installation argument](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-install-commands/src/setup/steps.rs#L139) · [Provisioning handoff](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-containerized-service/src/lib.rs#L195) · [Consumer](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-containerized-service/src/setup.rs#L456)

The receiver type and the code performing an operation can belong to different crates. Eight of the measured sites use a trait-provided default method. For example, kkt-entropy defines random_chars and the supplied implementation provides below, which that default calls. A useful map preserves both relationships. [Entropy method](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-entropy/src/lib.rs#L15)

### The 14 missing handoff chains

All 14 have identifiable source handoffs. They are engineering gaps in this prototype. Each row groups calls that lose the same part of the chain.

| Calls | Source chain and missing transfer |
| ---: | --- |
| 4 | kkt-web constructs GateForms, stores it in Site, and clones the field for an Axum state extractor. The receiver arrives through framework invocation of a handler or FromRequest. [Site and extractor](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-web/src/lib.rs#L89) |
| 1 | kkt-rules subscribe returns a boxed OperatorNotices. The web handler passes it to a stream. The stream holds and resupplies that state to an async closure which calls next. The missing transfer is through unfold. [Publisher](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-rules/src/operator_api.rs#L51) · [Stream](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-web/src/lib.rs#L1118) |
| 3 | HereLanes reads the deployment and returns a boxed LocalLanes or ContainerLanes through a fallible factory. The lane command unwraps it and invokes the selected server. Which type arrives depends on the deployment and command arguments. Both belong to kkt-operator-commands. [Factory](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/native.rs#L273) · [Calls](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-operator-commands/src/lanes.rs#L173) |
| 3 | Native credentials reserve wraps a successful file in CredentialFile inside Result::map. Agent register and reclaim obtain the boxed slot through error handling and save through a mutable receiver. The recovery slot also goes through Option::transpose. [Construction](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/credentials.rs#L29) · [Consumption](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-agent-commands/src/agent.rs#L44) |
| 3 | Native credentials claim_state wraps StateFile. The watch command uses map, transpose and as_deref to pass an optional borrowed Place. kkt-watch calls load and save when that state exists. [Construction](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/credentials.rs#L38) · [Forwarding](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-agent-commands/src/watch.rs#L39) |

## The boundary for an arbitrary project

| Question | What can be guaranteed |
| --- | --- |
| Which dependencies, resolved item references, impl declarations and call sites exist? | These compiler inventories are finite and checkable for a fixed successful build and defined set of bodies. A declared dependency can remain unused. Features, targets, tests and generated code can change the inventory. |
| Does B inject A directly or through an adapter into C? | Evidence for full injection is a value implementing C's contract passed from B to C. For adapted injection, B supplies its own implementation, and the relevant method calls A or forwards a value or effect from A. Where these handoffs are visible the mapper can discover the chain without a supplier name or expected route. A method that also calls a logging crate illustrates why a call edge alone cannot establish whose work the adapter delivers. |
| Which concrete supplier could a trait call receive? | In a closed build, rule out ineligible types and sometimes prove uniqueness without reconstructing all handoffs. For workspace traits the prototype keeps blanket and generic declarations as patterns. The independent test builds one blanket impl shared by concrete types from two different crates. A generated table establishes a type was materialized somewhere. It does not establish it reaches a particular call. |
| Can a library identify every downstream supplier? | An open trait object parameter may receive a type supplied by future callers. The test compiles the very same library with two applications. One gives it a left-crate provider, the other a right-crate provider. Both run successfully. From the library alone the complete answer ends at the parameter. |
| Can a mapper decide every feasible injection path inside a closed program? | No general algorithm can do this under the usual unbounded program model, even if every run on finite input terminates. Given an arbitrary computation, simulate one step per input byte. At EOF pass B to the consumer if the computation has halted, otherwise pass A. B reaches the consumer for some finite input exactly when that computation eventually halts. An always-correct mapper for this question would decide the halting problem. Passing the same provider through two different helpers on those branches gives the same limit for identifying every feasible handoff even when the supplier is already known. |
| When is exact feasibility attainable? | A closed transition system with explicitly bounded state and inputs admits exhaustive reachability. The independent test exercises the entire 256-value domain of a terminating function. Two providers get compiled and appear in the structural search. Only one can reach the consumer, because the other is chosen on odd inputs and the consumer runs only on even inputs. Larger finite bounds can make exhaustive exploration impractical. A recorded execution establishes the path taken on that run. |

A checkable product guarantee is to account for each in-scope use site and attach the evidence for each claim. A path in a specified finite graph, a provider forced by the build, an alternative selected by a known predicate, and an open handoff answer different questions. A call to a plugin or a value received from unexamined foreign code can end the static explanation at that boundary. A guarantee about an actual execution needs assumptions about its inputs and environment.

## Reproduction

Use the prerequisites in the compiler README. Run the independent tests and then measure the pinned checkout in a fresh directory.

```sh
node proof/compiler/test_balanced.ts
python3 proof/compiler/test_limits.py
CARGO_PROFILE_DEV_DEBUG=0 proof/compiler/run --dispatch-bounds /path/to/kashkeshet /tmp/kkt-limits -p kashkeshet --bin kkt --lib
python3 proof/compiler/test_kkt.py /tmp/kkt-limits
node proof/compiler/test_kkt_limits.ts /tmp/kkt-limits
```

The general analysis writes routes.json with call-specific traces, structural.json with candidate handoffs and limits.json with one evidence classification per site. The kkt-specific check writes audited-limits.json with the audited receiver crate and its basis. To reconcile the previous 375, run limits.py with the report directory and --prior-ledger pointing at the JSON ledger from compare.py, then repeat the kkt-specific check. It rejects a different set of source sites. To reproduce the login counterfactual, replace the single expression self.setup_parts(clock, login) with self.setup_parts(clock, &NoLogin) in the provision method in a disposable checkout, repeat the build in a separate report directory, and pass that second directory as the second argument to test_kkt_limits.ts.
