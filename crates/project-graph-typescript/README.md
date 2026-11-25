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
- **Files.** The files the tsconfig names, plus every workspace file they import. Files outside the workspace that they import are external.
- **Identity.** The revision is the Git commit. When there are local changes, it is `<commit>-dirty-<hash of the changes>`. A project outside any Git repository gets `unversioned-<hash>`. The hash covers every file the collector and the compiler read: the sources, declaration files, package manifests, the tsconfig and every config it extends. The TypeScript package's own files are left out. The graph ID ends with a hash of the graph's whole contents, so two graphs with one ID hold the same evidence.
- **Namespaces.** A `namespace` is an item of kind module, and its members are items named through it, such as `Paths.folder` or `Paths.Folder.path`; `namespace A.B {}` nests `B` in `A`. A member is exported when its namespace is and it says `export`. Ambient module declarations (`declare module "x"`) are not items.
- **Package entry.** Each package's entry is its source entry file. That is the `source` field, else `types` or `main` when it names a TypeScript source file, else the first of `src/index.ts`, `src/index.tsx` and `index.ts` that exists. Without one, visibility is unknown. An item is exported when the entry exports it.
- **Unbuilt packages.** A package's `exports` and `main` usually name built output, which a fresh checkout doesn't have. When the compiler can't resolve an import of a workspace package by its name, the collector resolves it to that package's source entry. So a project maps without being built, and with or without its dependencies installed.

## How it works

| Part | Responsibility |
| --- | --- |
| `collector/src/checker.ts` | Our interface to the checker: about ten calls. |
| `collector/src/typescript6.ts` | That interface on the TypeScript 6 API. Moving to TypeScript 7 replaces this file. |
| `collector/src/items.ts`, `facts.ts` | Walk the syntax tree and ask the checker about each name, call and value move. |
| `collector/src/records.ts` | The JSON Lines records the collector prints. `src/collection/records.rs` mirrors it. |
| `src/collection` | The `Collector` contract and `NodeCollector`, which runs the collector. |
| `src/assembly` | Turns the records into the graph. This is pure. |
| `src/routes` | Builds the routes from `project_graph` facts only. Value routes come from the shared `value-routes` tracer. |
| `src/revision` | The source revision, from Git. |

Imports, re-exports and references are facts only when they reach another package or something outside the project. Calls, implementations and value moves are facts everywhere, because routes pass through code inside one package.

A value moves between places. These are locals and parameters of a function, fields of a class or interface, a function's `return`, and the receiver of a method call, which moves into the called method's `self`. A class value written into an interface-typed slot is a `Coerce` move. If the class does not declare that interface, the same site also records a structural `Implements` fact. Array contents are tracked by element type. The elements of every array of `T` share one place, `{ owner: T, projection: "[element]" }`. An array literal stores into it. A `for...of` variable, the element parameters of a callback given to an array method (the first for `map`, `forEach`, `filter`, `find` and the like, both for `sort`, the second for `reduce`, whose accumulator is not followed; callbacks of other methods are not followed), and the result of `find`, `findLast`, `at`, `pop` and `shift` read from it. Like a field, it stands for every instance, so a hop through it is possible. Each side of `a || b` and `a ?? b` is a conditional move. A top-level constant written as an object literal of an interface type, such as `const tool: Tool = { ... }`, implements that interface.

A function-typed parameter `cb` of a function `f` has two slots: the arguments of every call through it, `cb(0)`, `cb(1)` and so on, and its result, `cb return`. Inside `f`, `cb(x)` passes `x` into `cb(0)`. Where `f` is called with a closure or a named function for `cb`, that function's parameters read from those slots, and its returned values move into `cb return`. The slots join every call of `f`, so hops through them are possible.

A closure (a function expression, arrow function or nested function that is not an item) has its own places, named `name@closure:<byte offset>`. When it uses a variable declared outside it, a `Capture` move takes the outer place into the closure's place, at the closure. A name bound by an object pattern reads the field it names from the pattern's type, so `{ backups }: InstallParts` reads `InstallParts.backups`. An array pattern over an array literal takes each element of the literal. Over any other value, each name reads from the element place of the value's type (see the arrays paragraph above), conditionally when the name has a default. A rest element (`...rest`) is not followed. Destructuring in an assignment, such as `({ a, b = d } = value)` or `[a, b] = value`, follows the same rules as in a declaration. Nested patterns are not followed.

When a move happens on one branch of a conditional, its hop is `Possible`, with the condition's text.

There are three kinds of route:

- **Direct use.** One hop from the supplier to the user, for an import, re-export, reference, implementation, call or declared dependency between packages.
- **Value route.** The shared `value-routes` tracer searches back from a method call's receiver to each `new` expression that can supply it, keeping one path per origin. A value returned from a call is matched to that call's own arguments. A move into or out of a field, or out of a closure's captured variable, stands for every instance of that place, so its hop is possible. If no origin is reached, the route stops at the call and says why.
- **Receiver bound.** For a call through an interface, one bound for each class with an `Implements` fact for that interface.

Not yet covered: arrays kept per variable rather than per element type, other containers such as `Map` and `Set`, and calls through function-typed values beyond recording them as indirect. A closure is followed when it is given as an argument for a parameter; one stored in a variable or a field and called later is not. A value that comes from outside the project is not an origin, so its trace stops.

## Sample

`sample/` is a small workspace used by `tests/sample_project.rs`:

- `backup-api` declares a `Backups` interface.
- `backup-os` implements it as `OsBackups`.
- `installer` holds a `Backups` and calls `write` on it.
- `app` builds the installer with either `OsBackups` or its own `MemoryBackups`, which implements `Backups` without saying so.
- `vendor/clock` stands for a third-party package.

## Mapping a real project

This is how [manypkg](https://github.com/Thinkmill/manypkg) (MIT, four packages) was mapped, with Claude Code answering the evaluation workbook. Keep the clone and the outputs outside this repository.

```sh
git clone https://github.com/Thinkmill/manypkg.git ~/src/manypkg
# Install dependencies without running their scripts. The project's own
# build step is not needed: imports between its packages resolve to source.
(cd ~/src/manypkg && npm install --ignore-scripts)

cargo build -p mapper
PATH="$PWD/target/debug:$PATH" srs map ~/src/manypkg \
    --language typescript --target tsconfig.json --configuration tsconfig \
    --evaluator manual --workbook ~/srs-out/manypkg/workbook --agent claude \
    --out ~/srs-out/manypkg/map.toml --page ~/srs-out/manypkg/map.html
```

`--agent claude` runs Claude Code in the project directory with read access and `srs eval`, so `srs` must be on `PATH`. On manypkg at 97e2ab9 the graph has 155 items, about 2,150 facts and no gaps, and the run takes about five minutes. Without Claude, leave out `--agent` and answer the workbook with `srs eval` (see the mapper README).
