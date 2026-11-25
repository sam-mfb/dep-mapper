# Libraries for project-graph-rust and project-graph-typescript

Spike, 2026-11-25. Question: how would we build the two language crates that produce a `ProjectGraph`, and which existing libraries can do part of the work?

## Recommendation

| Language | Build on | Write ourselves |
| --- | --- | --- |
| Rust | A rustc driver, as in `proof/`, run through Cargo's workspace wrapper on a pinned toolchain. Read function bodies through `rustc_public` (the in-tree "stable MIR" interface). Use the private compiler interface only for what `rustc_public` lacks: `use` items, re-exports, visibility and the unused-import lint. | The route tracer, and the mapping from compiler records to `Fact`s. |
| TypeScript | The TypeScript type checker. Start on the TypeScript 6 JavaScript API, behind a narrow interface of about ten checker calls. Move to the TypeScript 7 API once 7.1 is final. Do not adopt ts-morph. | Fact extraction from the syntax tree plus checker, and the route tracer. |

No library we can use builds routes. CodeQL comes closest, but its license rules it out (see below). Both language crates therefore need their own value-flow tracer. The tracer in `proof/` already works over a small set of value moves: construct, store, read, pass, return, coerce, capture. If both crates emit facts in those terms, one tracer can serve both. Whether it lives in a shared crate or in `project-grapher` is a design choice for later.

## What the crates must produce

From `crates/project-graph`:

- **Modules and units.** Packages, and the build units analyzed for each.
- **Items.** Declared types, values, functions and methods, with visibility and source span.
- **One-hop facts.** Imports (marked unused, type-only or value), re-exports, type and value references, implements, and calls with dispatch kind. Also receiver bounds and value moves.
- **Routes.** Chains of value moves from where a value is constructed to the call that uses it. Each chain ends at the use or at a recorded stopping point.

Items and one-hop facts are what compilers and indexers already know. Routes need interprocedural value flow, which only whole-program analysers offer.

## What `proof/` already established

`proof/compiler` is a rustc driver (`collect.rs`). It reads high-level IR for paths and `use` items, and reads typed MIR (the compiler's intermediate representation of function bodies) before optimization for calls and value moves. Node scripts (`report.ts`, `structural.ts`) then trace routes over the emitted JSON. On kashkeshet it accounted for all 668 virtual calls to workspace contracts. It found a call-matched chain at 391 of them.

The `.ts` files in `proof/` analyze the Rust output; they are not a TypeScript-language analyzer. No earlier work exists on TypeScript subjects. The TypeScript recommendation below rests on this spike's own probe.

## Rust options

| Option | Items and one-hop facts | Routes | Stability and upkeep | Packaging | License |
| --- | --- | --- | --- | --- | --- |
| **rustc driver, private API** (proof's approach) | All of it: HIR paths, `use` items and visibility, lints such as unused imports, trait impls, MIR calls with compiler-selected implementations. | Full typed MIR before optimization to trace over. | No stability. Breaks with every toolchain, so pin one exact version. | Binary built against the `rustc-dev` component of the pinned toolchain. Cargo runs it via `RUSTC_WORKSPACE_WRAPPER`. The subject must build with that toolchain. | MIT OR Apache-2.0 |
| **`rustc_public`** (formerly stable MIR) | Crate items, trait impls, spans, MIR calls with `Instance::resolve`. No HIR: `use` items and re-exports are invisible. | MIR bodies, but only optimized ones (see experiment). | Still marked "completely unstable". Lives in-tree. Not on crates.io; publishing is an accepted 2026–27 project goal, staffed part time ([goal](https://goals.rust-lang.org/2026/rustc-public.html)). | Same as the private API. Ships inside `rustc-dev`; it is present in 1.98.1. | MIT OR Apache-2.0 |
| **Charon** | Types, functions, traits, impls and spans, exported as a `.llbc` JSON file. No `use` items. | Simplified MIR (ULLBC) as data, readable from any language. | Self-described beta with breaking changes. Nightly prereleases; the latest is 2026-11-25. | CLI pinned to `nightly-2026-09-17`, so the subject must build on that nightly. Unsupported features become gaps. | Apache-2.0 |
| **rust-analyzer crates** (`ra_ap_*`) | Good: name resolution, imports, references, impls, method resolution. It has its own trait solver and macro expander, which can differ from rustc's. | It has an internal MIR for const evaluation and borrow checking, not offered as an analysis API. | Version 0.0.x, released weekly with no semver promise ([crates.io](https://crates.io/crates/ra_ap_hir): 0.0.363, 2026-11-16). | Ordinary crates on stable Rust. No pinned compiler, but proc macros need a separate server, and build configuration is set by hand, not taken from a real build. | MIT OR Apache-2.0 |
| **SCIP** (`rust-analyzer scip`) | Definitions, references, import roles and implementation relationships. | None. Calls are bare references. | Format maintained ([scip-code/scip](https://github.com/scip-code/scip)). | CLI output file. | Apache-2.0 |
| **rustdoc JSON** | Public items, impls and signatures. | None. No bodies. | Nightly only; format version changes often (now 61). | Compiler flag. | MIT OR Apache-2.0 |
| **CodeQL** | Yes. | Real global data flow. Rust support is GA since CLI 2.23.3 ([changelog](https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available)). | Maintained by GitHub. | CLI builds a database; queries in QL. | **Disqualifying.** Free only for open-source code on GitHub, academic research and query testing. Use on private code, in CI, or as a service for others needs GitHub Advanced Security ([license](https://github.com/github/codeql-cli-binaries/blob/main/LICENSE.md)). |
| **Joern** | A `rust2cpg` frontend exists in the repo but is undocumented. | Code property graph with data-flow queries. | Very active (v4.0.692, 2026-11-25). | JVM application with a Scala query language. | Apache-2.0 |
| MIRAI, rustc_plugin, Flowistry | MIRAI is archived. rustc_plugin (MIT) is a helper for writing drivers like ours, not an analyzer. | — | — | — | — |

### Experiment: what `rustc_public` sees

I wrote a 50-line driver using `rustc_public`'s `run!` macro on Rust 1.98.1. The fixture had a trait, an implementation, a struct with a `Box<dyn Trait>` field, a direct call, a virtual call, a small helper, an unused import and a re-export.

- **Calls.** The virtual call was reported with its vtable slot. The direct trait call resolved to `OsBackups::write`. Each call had its operands and a line span.
- **Value moves.** The coercion of `Box<OsBackups>` to `Box<dyn Backups>` and the aggregate building `Parts` were both visible.
- **Imports.** Neither the unused import nor the re-export appears anywhere in the interface. The compiler's own lint still warned about the import.
- **Optimization.** `rustc_public` returns optimized MIR. At `-C opt-level=3`, inlining removed the call to `helper` inside `write` and the call to `direct` in `main`. At `opt-level=0` all calls remained. The driver must force opt-level 0. I did not test whether also passing `-Zmir-opt-level=0` is needed for larger crates.

### Rust recommendation

Keep the driver. Only a compiler-backed tool gives the facts the build actually used: selected implementations, macro-expanded code and the unit's features. Nothing else in the table gives that with usable data flow and an acceptable license.

Split the driver in two. Code that reads bodies uses `rustc_public`: calls, coercions, moves and trait impls. The rest uses the private API: `use` items, re-exports, visibility and lints. Hide both behind a contract owned by project-graph-rust. Each toolchain bump then changes only that module, and the `rustc_public` half should become a crates.io dependency when the project goal lands.

The pinned toolchain is a real cost. The subject must compile on our pinned version, with `RUSTC_BOOTSTRAP=1` to unlock the driver's unstable features. A project that needs a newer compiler waits for us to bump the pin.

Cargo's `--unit-graph` is also unstable. The wrapper sees each rustc invocation, including its crate name, `--cfg` flags and `--extern` dependencies. Units and their edges could be read from those, leaving only `cargo metadata`, which is stable.

rust-analyzer is the fallback if the toolchain pin proves unacceptable. It would give items and one-hop facts on any toolchain, but routes would need our own body lowering. That is a much larger project.

## TypeScript options

| Option | Items and one-hop facts | Routes | Stability and upkeep | Packaging | License |
| --- | --- | --- | --- | --- | --- |
| **TypeScript 6 compiler API** (`typescript@6.0.3`) | All of it (see experiment). | Typed syntax tree to trace over. Resolves each call to its declaration. | 6.0 is the last JavaScript-based line. Its API is long established but receives no new development. | npm package run in Node. | Apache-2.0 |
| **TypeScript 7 API** (`typescript@7.x`, native Go compiler) | The same checker queries, sent to a native `tsgo` process. | Same as above. | 7.0.2 (2026-07-08) exposes it under `typescript/unstable/*`. 7.1.0-beta (2026-10-20) moved it to `typescript/sync` and changed it incompatibly. Microsoft's 7.0 announcement promised "a new (and different) API" in 7.1 ([announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)). | npm package with a per-platform native binary. The client library is JavaScript; the wire protocol is published in `typescript/proto`. | Apache-2.0 |
| **ts-morph** 28.0.0 | Convenience wrapper over the TS 6 API. | Same as TS 6. | Maintained. It bundles TypeScript 6.0.2, a version behind, and has announced no TS 7 plan. | npm. | MIT |
| **scip-typescript** 0.6.2 | Definitions, references, implementation relationships. | None. | Maintained (2026-11-10). | CLI. | Apache-2.0 |
| **CodeQL** (JS/TS) | Yes. | Real global data flow. | Maintained. | CLI. | Same disqualifying license as for Rust. |
| **Joern** `jssrc2cpg` | Parses with a Babel-style tool. Types are partly from the TypeScript compiler and partly recovered heuristically ([docs](https://docs.joern.io/frontends/javascript/)). | Data-flow queries, with weaker dispatch than a real checker. | Active. | JVM. | Apache-2.0 |
| **Jelly** 0.13.0 | Call graph and points-to analysis for JavaScript and TypeScript. | Approximate "which objects reach this call" answers. How it uses TypeScript types is unverified. | Last push 2026-08. Academic project. | npm CLI. | BSD-3-Clause |
| oxc, swc, Biome, dependency-cruiser | Parsing, scopes, module imports. | None. No type checker, so no dispatch or interface resolution. | — | — | — |

### Experiment: the same probe on TypeScript 6, 7.0 and 7.1 beta

The fixture mirrored the Rust one: an interface, a class implementing it, a class holding an interface-typed field, a direct call, an interface call, an unused import, a type-only import and a re-export. The probe walked the syntax tree and asked the checker about each import, heritage clause, call and argument.

TypeScript 6.0.3 and the 7.0.2 unstable API gave the same answers:

- Each import and re-export resolved to its declaration through the alias. The type-only import was marked as such.
- The unused import was reported by diagnostic 6133 (with `noUnusedLocals`).
- `p.backups.write()` resolved to `Backups.write`, the interface method. `new OsBackups().write()` resolved to `OsBackups.write`.
- Each argument's type was shown against its parameter's type. `OsBackups` passed into a parameter typed `Backups` is TypeScript's equivalent of a Rust coercion to `dyn`: the place where an implementation enters an interface-typed slot.
- `new OsBackups()` with no written constructor has no declaration. The fact must point to the class instead.

The 7.0.2 probe failed on 7.1.0-beta in three places: import paths, snapshot creation and node identity. The TS 7 API works today, but each minor release is likely to break it until 7.1 is final.

### TypeScript recommendation

Use the TypeScript checker; no lighter tool resolves interface calls or aliases. Run it in a Node child process that project-graph-typescript starts, with the TypeScript version pinned by a lockfile. The child streams facts as JSON Lines, as the Rust driver does.

Start on TypeScript 6. The probe used the same ten or so checker calls on both versions, so moving to the TypeScript 7 API is a contained change once 7.1 is final. TypeScript 6 will eventually fall behind syntax and configuration that only TypeScript 7 accepts, so the move should not wait long. ts-morph adds editing conveniences we don't need and lags TypeScript releases.

Two language differences matter for facts and routes:

- **Implementation is structural.** A class satisfies an interface without saying `implements`. Heritage clauses give some `Implements` facts. The rest must come from handoff sites where a class-typed value enters an interface-typed slot, which the checker reports as assignable.
- **The compiler never selects an implementation.** Every method call is resolved to a declaration, not to a runtime target. A call on an interface type is `Dispatch::Contract`. A call on a class type can still reach a subclass override. Receiver bounds therefore always come from route evidence, never from the compiler.

## Not verified

- Whether rust-analyzer now uses the next-generation trait solver by default.
- The maturity of Joern's `rust2cpg` and Jelly's use of TypeScript types.
- The final shape of the TypeScript 7.1 API, and whether a Rust client could speak its protocol directly without Node.
- How `rustc_public`'s optimized MIR differs from the proof's pre-optimization MIR on a real workspace, at opt-level 0.
