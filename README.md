# sharsheret (srs)

Sharsheret maps a project's packages and how their work reaches each other. It reads one build of a Rust or TypeScript project and records what each package exposes, who uses it, and the routes values take between packages. A person, an agent such as Claude Code, or a language model then judges what each package is for and what its connections mean, citing the code for each answer. The result is a map you can open in the browser and follow from an arrow down to the source that explains it.

`docs/architecture/decisions.md` explains the design. `crates/mapper/README.md` is the reference for the `srs` commands.

## Setup

- Rust through `rustup`. `rust-toolchain.toml` pins the compiler and installs the `rustc-dev` component the Rust grapher needs.
- For TypeScript projects: Node 22.18 or later, and the collector's dependencies, installed once:

  ```sh
  (cd crates/project-graph-typescript/collector && npm ci)
  ```

- Optional: Claude Code on your `PATH`, to have it answer the manual evaluator's questions. For the LLM evaluator, a Claude API key in `ANTHROPIC_API_KEY`.

Build the program and check the workspace:

```sh
cargo build -p mapper          # the program is target/debug/srs
cargo test --workspace         # the TypeScript tests need the npm ci step above
```

## A tour

Each step uses only what the steps before it set up. Commands run from the repository root.

### 1. Look at sharsheret's own map

`docs/architecture/sharsheret-map.toml` is the map of this repository, made by the commands in step 2. Open it without any analysis:

```sh
target/debug/srs show docs/architecture/sharsheret-map.toml --serve 127.0.0.1:8080
```

Then visit http://127.0.0.1:8080/. Select a module to see its concern and its connections. Select a route to follow it hop by hop to the source lines behind it. `--page map.html` writes the page to a file instead.

### 2. Map a Rust project and judge it yourself

This maps sharsheret again. The first run analyzes the build and writes a workbook of questions, then stops because nothing has been answered yet:

```sh
target/debug/srs map . --language rust --target mapper --configuration "default features" \
    --evaluator manual --workbook target/srs-self/workbook \
    --out target/srs-self/map.toml --page target/srs-self/map.html
```

Routes that start in `std`, `serde`, `tokio` and the other crates that shape the language are left out; the map's coverage gaps say how many. `--exclude` changes the list, and `--exclude ''` keeps everything.

Read and answer the workbook with `srs eval`. Each answer is checked when you record it, and a refused answer says why:

```sh
eval="target/debug/srs eval --workbook target/srs-self/workbook"
$eval index                          # the pieces and how far each has got
$eval show module/map-web            # one module: its routes, grouped by where they lead, and its items
$eval judge module:map-web --concern "Presents a map in the browser." --kind logic \
    --cluster mapping --reason "It turns a ProjectMap into a page." --cite <a handle from the piece>
$eval clusters mapping               # name every cluster you used, top to bottom
$eval submit
```

You don't have to answer everything: modules and routes you leave alone stay on the map as unreviewed. Run the same `srs map` command again. It resumes the workbook, builds the map, and writes the page.

To have Claude Code answer instead, add `--agent claude` and put `srs` on your `PATH` (`PATH="$PWD/target/debug:$PATH"`). It answers the whole workbook in one run of a few minutes. That is how `docs/architecture/sharsheret-map.toml` was made.

### 3. Map a TypeScript project

`crates/project-graph-typescript/sample` is a small npm workspace. Map it the same way, with `--agent claude` or by answering it yourself:

```sh
target/debug/srs map crates/project-graph-typescript/sample --language typescript \
    --target tsconfig.json --configuration tsconfig \
    --evaluator manual --workbook target/srs-sample/workbook \
    --out target/srs-sample/map.toml --page target/srs-sample/map.html
```

### 4. Let a model judge

`--evaluator llm` sends each piece to Claude and checks its answers by the same rules. It needs `ANTHROPIC_API_KEY`; without it, `srs map` stops before analysis and says so.

```sh
target/debug/srs map crates/project-graph-typescript/sample --language typescript \
    --target tsconfig.json --configuration tsconfig --evaluator llm \
    --out target/srs-llm/map.toml --page target/srs-llm/map.html
```

### 5. Plan a change with a draft

A draft is a file of proposed modules, dependencies and routes, either standing alone or laid over a saved map. `docs/architecture/sharsheret.toml` is the project's hand-written architecture draft:

```sh
target/debug/srs draft check docs/architecture/sharsheret.toml     # problems, with line and column
target/debug/srs draft render docs/architecture/sharsheret.toml --out target/draft.html
target/debug/srs draft preview docs/architecture/sharsheret.toml   # http://127.0.0.1:8000/, reloads on save
```

Edit the draft while the preview runs, and the page reloads. If the draft has a problem, the preview keeps the last good page and shows the problem.

## Where to read more

| Topic | Document |
| --- | --- |
| Design, scope and the three core types | `docs/architecture/decisions.md` |
| Every `srs` command and option | `crates/mapper/README.md` |
| The saved-map and draft file format | `crates/map-persist/FORMAT.md` |
| How TypeScript projects are read | `crates/project-graph-typescript/README.md` |
| The earlier compiler experiments | `proof/README.md` |
