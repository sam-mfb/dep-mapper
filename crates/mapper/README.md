# mapper: the `srs` program

`srs map` analyzes one build of a project, has it evaluated, and saves and shows the resulting map. `srs eval` is how a person or an agent answers the manual evaluator's questions.

## `srs map`

```sh
srs map <root> --language <rust|typescript> --target <target> --configuration <settings> \
    --evaluator <manual|llm> [--workbook <dir>] --out <map.toml> [--page <map.html>] [--serve <address>]
```

| Option | Meaning |
| --- | --- |
| `<root>` | The directory holding the project's source. |
| `--language` | Selects the grapher. TypeScript is available now. Rust will be once project-graph-rust merges. |
| `--target`, `--configuration` | The build to analyze, in the grapher's terms. For TypeScript, the target is the tsconfig file, relative to the root, and the configuration is `tsconfig`. |
| `--evaluator` | Who judges the graph. `manual` needs `--workbook`. The LLM evaluator is not available yet. |
| `--out` | Where to save the map. map-persist writes it, and it can be reopened without analysis. |
| `--page` | Also write the map-web page to this file. |
| `--serve` | Also serve the page at this address, such as `127.0.0.1:8080`, until stopped. |

Problems the map builder finds while joining the evaluation to the graph are printed as notes. The map is built without them.

## The manual evaluator and `srs eval`

The manual evaluator writes a workbook into the `--workbook` directory: instructions, one readable piece per module and per pair of modules, and the answers so far. For now `srs map` stops after writing it, with a message that the answers are not submitted. Answer with `srs eval`, then run the same `srs map` command again. It resumes the workbook and, once the answers are submitted, builds the map.

```sh
srs eval --workbook <dir> help        # the answer commands
srs eval --workbook <dir> index       # the pieces and how far each has got
srs eval --workbook <dir> show module/<name>
srs eval --workbook <dir> judge module:<name> --concern ... --kind ... --cluster ... --reason ... --cite ...
srs eval --workbook <dir> submit
```

`srs eval` exits with 0 when the command did its work, 1 when an answer was refused or a check found problems (the output says why), and 2 when the command could not run.

## Example: the TypeScript sample

After `npm ci` in `crates/project-graph-typescript/collector` (the collector needs Node 22.18 or later):

```sh
cargo build -p mapper
srs=target/debug/srs
$srs map crates/project-graph-typescript/sample --language typescript --target tsconfig.json \
    --configuration tsconfig --evaluator manual --workbook /tmp/srs-sample/workbook \
    --out /tmp/srs-sample/map.toml --page /tmp/srs-sample/map.html
# answer with: $srs eval --workbook /tmp/srs-sample/workbook ...; then submit
# and run the same srs map command again
```

`tests/typescript_manual.rs` runs these steps with a few answers and checks the saved map.
