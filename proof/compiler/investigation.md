# Can we find the supplier, and can we explain the choice?

**On the same 668 written kkt contract calls, coverage increased from 173 to 293.** A counted site has at least one source-backed path to a concrete provider. The new analysis recovered 120 of the 495 misses, retained a provider at all 173 previous sites, and leaves 375 unresolved. Of the 293 with an answer, 62 also have an unresolved incoming path. These are comparisons on one pinned Linux build.

## What made a difference

| Rules enabled | Calls with a provider, out of 668 |
| --- | ---: |
| Original analysis | 173 |
| Add closure and async capture tracing alone | 266 |
| Add standard smart-pointer operations alone | 177 |
| Use both | 292 |
| Also follow enum payloads and concrete generic arguments | 293 |
| Also check branch conditions | 293 |

The async case is common in kkt. Calling an async function builds a future. The work runs later, using values stored inside that future. Rust records which field holds each captured value. Following that field back to construction finds the install command's output object. Following selected standard pointer operations also finds the clock that writes the log file's timestamp. The pointer rule recognizes the standard implementations of borrowing the pointee, cloning a shared pointer and constructing these holders. A custom implementation still requires its own body. [Install command](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-install-commands/src/lifecycle.rs#L220) · [Clock consumer](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-rolling-file/src/lib.rs#L67) · [Clock supplier](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/commands/serve.rs#L106)

Enum payload tracking adds one site on this build. It also helps diagnose other paths that previously stopped at a field read. Following a generic parameter back to a call with a concrete type passes its independent test and adds zero kkt sites. The two large gains interact: some paths need both capture and pointer tracing.

## Accounting for the misses

Of the original 495 misses, 297 stopped solely at a missing caller, 147 solely at a returned value whose body was unavailable, 48 solely at another operation, and three at multiple types of boundary. After applying the new rules, the following groups account for all 375 remaining misses. Each call appears once.

| Where the remaining trace stops | Sites |
| --- | ---: |
| No indexed caller for the function that received the value | 193 |
| No indexed body returning the value | 78 |
| Both of the preceding boundaries on different incoming paths | 2 |
| Conversion to a trait object with a generic or otherwise unresolved concrete type | 47 |
| A value operation or definition whose origin is untracked | 37 |
| The value is itself returned by a trait call whose result is untraced | 18 |

For example, the container service passes a reference to its generic projects field. The compiler knows the field's contract. Identifying its provider requires the concrete type in the relevant invocation. Elsewhere, a handler can be registered with a framework and invoked indirectly. Finding a matching implementation elsewhere in the project does not establish that the framework called it with this value. A further coverage experiment should join a known virtual invocation to that specific implementation body **while preserving the receiver and arguments from the same invocation**. These counts do not predict how many sites such a rule would recover. [Generic projects field](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-containerized-service/src/lib.rs#L165)

## Which provider does the condition select?

* **Real runtime choice.** For the invocation in kkt's admission decision, the analysis identifies three pairings: the Open setting supplies Open, SharedKey supplies the shared-key provider, and Bearer supplies AnyBearer. The setting on a particular server is runtime data. [Selection in kkt](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/crates/kkt-admission/src/lib.rs#L191)
* **Choice already made by the build.** The unfiltered analysis showed systemd, launchd and the unsupported fallback at 21 service call sites. Checking the compiled target conditions leaves only systemd at all 21 on this Linux build. That reduces the number of kkt-contract sites with several provider types from 31 to 10. [Build conditions](https://github.com/sam-mfb/kashkeshet/blob/41fbc289dbfe68be90840d107e217762229b9745/src/machine/service.rs#L10)
* **Repeated or independent flags.** In the independent workspace, a provider is selected in one function and invoked in another. Following the same immutable Boolean across that boundary rules out a provider that would require the flag to be both true and false. Negating the flag and using two independent flags are separate checks. The executable exercises all four independent combinations.
* **Limit.** A reported condition is required to reach a particular step in the witness. It can describe when the consumer runs even when it does not distinguish its providers. Unknown predicates, mutable state, callbacks, loops and external input can still leave several structural paths. The analysis does not solve arbitrary Boolean expressions or prove every reported path executable.

The implementation checks which executable control-flow edges a block requires. It drops borrow-checker-only edges, respects known build-time Boolean values, and correlates immutable Boolean arguments across matched calls. Enum labels come from the compiler's discriminant-to-successor table. It emits explicit contradictory-path evidence when it can rule out a candidate.

## Checking the result

The independent fixture covers an async capture, a shared pointer, a custom dereference selecting a different field, enum payloads, and two instantiations of the same generic helper. It checks both runtime selections against the same executable and switches a compile-time feature in a second build. The kkt rewiring test changes only the install output slot in a disposable checkout and reroutes the Bearer arm to Open. The provider traces must change in those places while other NativeOutput uses and the SharedKey route survive. These checks validate particular witnesses and exclusions. The 293-site total is not a precision estimate.

Use the build command in the parent README, then run:

```sh
python3 proof/compiler/test.py
python3 proof/compiler/test_kkt.py /tmp/kkt-report
CARGO_PROFILE_DEV_DEBUG=0 python3 proof/compiler/test_rewire.py /path/to/pinned/kashkeshet
python3 proof/compiler/compare.py /tmp/original-report /tmp/kkt-report --ledger /tmp/sites.json
```

Build the original report with mapper commit 966b900, the same kkt revision 41fbc289 and the same Cargo selection. The comparison checks identical call-site identities. It lists the old and new provider types, source span and stopping locations for each site. For a rule-by-rule replay, use a fresh directory containing links to the new report's facts directory and units.json, and run report.ts there with any combination of --without-captures, --without-pointers, --without-variants, --without-generics and --without-guards.
