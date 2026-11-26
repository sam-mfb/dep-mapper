# mapper: the `srs` program

`srs map` analyzes one build of a project, has it evaluated, and saves and shows the resulting map. `srs show` shows a saved map again. `srs eval` is how a person or an agent answers the manual evaluator's questions.

## `srs map`

```sh
srs map <root> --language <rust|typescript> [--cache <dir>] [--exclude <list>] \
    --target <target> --configuration <settings> \
    --evaluator <manual|llm> [--workbook <dir>] [--agent <none|wait|claude>] \
    --out <map.toml> [--page <map.html>] [--serve <address>]
```

| Option | Meaning |
| --- | --- |
| `<root>` | The directory holding the project's source. |
| `--language` | Selects the grapher: project-graph-rust or project-graph-typescript. |
| `--cache` | For Rust: where the grapher keeps its rustc driver and build output between runs. Without it, `<root>/target/srs`. The first run builds the driver, which takes about a minute. |
| `--exclude` | Outside crates or packages whose routes the graph leaves out, separated by commas. It replaces the default list. For Rust the default is `std`, `core`, `alloc`, `thiserror`, `futures`, `serde`, `serde_json`, `tokio` and `tokio-util`: the toolchain and the crates that shape the language in our Rust principles. For TypeScript it is empty. `--exclude ''` keeps every route. The graph and the map's page record the list and how many routes and facts it removed, as a coverage gap. |
| `--target`, `--configuration` | The build to analyze, in the grapher's terms. For Rust, the target is a package, optionally with one target as `package/bin:name`, and the configuration is `default features` or Cargo flags such as `--features x`. For TypeScript, the target is the tsconfig file, relative to the root, and the configuration is `tsconfig`. |
| `--evaluator` | Who judges the graph. `manual` needs `--workbook`. `llm` asks Claude and needs `ANTHROPIC_API_KEY`; without it, `srs map` stops before analysis. |
| `--agent` | For `manual`: who answers while `srs map` runs. `none` (the default when the option is left out) stops after writing the workbook. `wait` shows the request and checks every 5 seconds for a submission, for up to 4 hours. `claude` runs Claude Code in the project directory; `srs` must be on `PATH` for it to run `srs eval`. |
| `--out` | Where to save the map. map-persist writes it, and it can be reopened without analysis. |
| `--page` | Also write the map-web page to this file. |
| `--serve` | Also serve the page at this address, such as `127.0.0.1:8080`, until stopped. |

Problems the map builder finds while joining the evaluation to the graph are printed as notes. The map is built without them.

## `srs show`

```sh
srs show <map.toml> [--page <map.html>] [--serve <address>]
```

Reopens a map saved by `srs map --out`, without analysis or evaluation, and writes or serves its page. For example, the checked-in map of sharsheret: `srs show docs/architecture/sharsheret-map.toml --serve 127.0.0.1:8080`.

## The LLM evaluator

`--evaluator llm` sends each module piece, the clusters and each page of routes to Claude (graph-evaluator-llm's recommended model and settings) with excerpts of the cited source, at most 40 lines each. A request may take up to 10 minutes, and one that fails for a passing reason is retried after 2, 8 and 30 seconds. Progress goes to standard error. Answers the evaluator's checks refuse go back to the model for repair; if they still fail, `srs map` stops and names the piece.

## The manual evaluator and `srs eval`

The manual evaluator writes a workbook into the `--workbook` directory: instructions, one readable piece per module and per pair of modules, and the answers so far. Someone answers it with `srs eval` and finishes with `srs eval ... submit`. With `--agent wait` or `--agent claude`, one `srs map` run waits for that and then builds the map. With `--agent none`, `srs map` stops after writing the workbook, with a message that the answers are not submitted. Run the same command again after they are: it resumes the workbook and builds the map. A run that stops without a submission keeps the answers recorded so far.

```sh
srs eval --workbook <dir> help        # the answer commands
srs eval --workbook <dir> index       # the pieces and how far each has got
srs eval --workbook <dir> show module/<name>
srs eval --workbook <dir> judge module:<name> --concern ... --kind ... --cluster ... --reason ... --cite ...
srs eval --workbook <dir> submit
```

`srs eval` exits with 0 when the command did its work, 1 when an answer was refused or a check found problems (the output says why), and 2 when the command could not run.

## Example: sharsheret itself

The Rust grapher needs the pinned toolchain from `rust-toolchain.toml`, with its `rustc-dev` component. From the repository root:

```sh
cargo build -p mapper
target/debug/srs map . --language rust --target mapper --configuration "default features" \
    --evaluator manual --workbook target/srs-self/workbook --agent claude \
    --out target/srs-self/map.toml --page target/srs-self/map.html
```

The `mapper` package depends on every other crate, so its build covers the whole project. The default `--exclude` list applies, so routes from `std`, `serde`, `tokio` and the other crates that shape the language are left out. With `--agent claude`, Claude Code answers the workbook. With `--agent none`, answer it yourself with `srs eval` and run the command again. `tests/sharsheret_rust.rs` runs the same steps for the smaller `map-builder` package, with a few answers.

`docs/architecture/sharsheret-map.toml` is the saved result of this command with `--agent claude` and `target/debug` on `PATH`, run on commit 9ab9a2f with the default `--exclude` list; the map records the commit and the list in its `[source]` table. Claude Code judged all 17 modules in five clusters (graph, evaluation, map, presentation, program), recorded the relationships between modules, and set aside the routine routes, such as derived serde code. To look at it without analysis:

```sh
target/debug/srs show docs/architecture/sharsheret-map.toml --serve 127.0.0.1:8080
```

To refresh it after the code changes, run the command above again with `--out docs/architecture/sharsheret-map.toml` and an empty workbook directory. A workbook for an older commit is refused rather than reused, since its answers cite another graph. `tests/checked_in_map.rs` checks that the checked-in map still loads, fits together and renders with `srs show`, so a change to the saved format or the map's rules that breaks it fails the tests.

## Example: the TypeScript sample

`srs` runs the TypeScript collector from the checkout it was built from, so run `npm ci` in that checkout's `crates/project-graph-typescript/collector` first, even for an `srs` copied elsewhere. The collector needs Node 22.18 or later.

```sh
cargo build -p mapper
srs=target/debug/srs
$srs map crates/project-graph-typescript/sample --language typescript --target tsconfig.json \
    --configuration tsconfig --evaluator manual --workbook /tmp/srs-sample/workbook \
    --out /tmp/srs-sample/map.toml --page /tmp/srs-sample/map.html
# answer with: $srs eval --workbook /tmp/srs-sample/workbook ...; then submit
# and run the same srs map command again
```

`tests/typescript_manual.rs` runs these steps with a few answers and checks the saved map, both as two runs and as one run with `--agent wait`.
