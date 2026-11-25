# project-graph-typescript

Builds the `ProjectGraph` for a TypeScript project with the TypeScript checker. It implements the `ProjectGrapher` contract from `project-graph`.

## Setup

The checker runs in a Node child process. It needs Node 22.18 or later, which runs `.ts` files directly. Install the collector's pinned dependencies once:

```sh
cd crates/project-graph-typescript/collector
npm ci
```

`cargo test` fails with a message naming this step if it was skipped.

## What it reads

- **Project.** An npm workspace. The root `package.json` lists the packages in `workspaces`, either as directories or as a directory followed by `/*`. Each package is a module of the graph with one build unit.
- **Selection.** `target` is the tsconfig file, relative to the project root. It names the source files and compiler options. `configuration` must be `"tsconfig"`, meaning the options exactly as that file states them.
- **Package surface.** An item is exported when the package's entry file exports it. The entry file comes from `types` or `main` in the package's `package.json`. Without either, visibility is unknown.

## How it works

| Part | Responsibility |
| --- | --- |
| `collector/src/checker.ts` | Our interface to the checker: about ten calls. |
| `collector/src/typescript6.ts` | That interface on the TypeScript 6 API. Moving to TypeScript 7 replaces this file. |
| `collector/src/items.ts`, `facts.ts` | Walk the syntax tree and ask the checker about each name, call and value move. |
| `collector/src/records.ts` | The JSON Lines records the collector prints. `src/collection/records.rs` mirrors it. |
| `src/collection` | The `Collector` contract and `NodeCollector`, which runs the collector. |
| `src/assembly` | Turns the records into the graph. This is pure. |
| `src/routes` | Builds the routes from `project_graph` facts only. |
| `src/revision` | The source revision, from Git. |

Imports, re-exports and references are facts only when they reach another package or something outside the project. Calls, implementations and value moves are facts everywhere, because routes pass through code inside one package.

A value moves between places. These are locals and parameters of a function, fields of a class or interface, a function's `return`, and the receiver of a method call, which moves into the called method's `self`. A class value written into an interface-typed slot is a `Coerce` move. If the class does not declare that interface, the same site also records a structural `Implements` fact. When a move happens on one branch of a conditional, its hop is `Possible`, with the condition's text.

There are three kinds of route:

- **Direct use.** One hop from the supplier to the user, for an import, re-export, reference, implementation, call or declared dependency between packages.
- **Value route.** Traced backward from a method call's receiver to the `new` expression that built the value, or to something outside the project. If the trace finds no incoming value, the route stops and says why.
- **Receiver bound.** For a call through an interface, one bound for each class with an `Implements` fact for that interface.

Not yet covered: values captured by closures, destructuring, namespaces, and calls through function-typed values beyond recording them as indirect.

## Sample

`sample/` is a small workspace used by `tests/sample_project.rs`:

- `backup-api` declares a `Backups` interface.
- `backup-os` implements it as `OsBackups`.
- `installer` holds a `Backups` and calls `write` on it.
- `app` builds the installer with either `OsBackups` or its own `MemoryBackups`, which implements `Backups` without saying so.
- `vendor/clock` stands for a third-party package.
