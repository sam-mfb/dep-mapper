# Tracing direct and adapted injection on kashkeshet

The subject is sam-mfb/kashkeshet at 4c520c70618845c90fd62c0e3c6a9961cbf3dbff (the next branch when pinned). The experiment targets the default Linux kkt binary and its libraries with Rust 1.98.1. It uses a local checkout of this exact revision.

## Expected answers from source

| Question | Expected answer | Must exclude |
|---|---|---|
| Implements and references | OsBackups implements Backups. Its implementation also takes Layout, reads DATABASE_FILE and calls old_names. | An ordinary constant or function reference becoming another implementation relationship. |
| Calls and supplies | Serve uses OsBackups directly to write and prune. NativeParts supplies its BACKUPS static in InstallParts.backups. Local backup and service upgrade use it through local_backup. | The Docker backup or Docker upgrade branch calling local_backup. |
| Callback | OsBackups::new receives the program’s database::copy. It stores that function. Its write method forwards it to write, then fill invokes it. | Prune invoking the copy function. |
| Shared identity | KeyMode enters Parts::new. The receiver stores identify and supplies a clone to the MCP endpoint and another to AdmissionGate, which invokes operator. | A fake product’s identity appearing on the kkt route. |
| Adapters | Probe::key_file uses OsKeyFiles; Probe::answer uses HttpProber. On the Docker lane-import path SetupLanes supplies ContainerLanes to the operator helper, and ContainerLanes calls ServiceContainer. | Assigning the HTTP operation to key_file or treating LocalLanes as the Docker provider. |
| Choices | The log writer is optional at runtime. for_this_os chooses Systemd for this build, with other choices fixed by different build targets. | Claiming that every serve run opens a file or the Linux binary chooses launchd at runtime. |

The executable proof covers the backup rows and a second example of adapted injection from kkt connect. The program supplies ClaudeCommand through the ClientCommand contract in AgentParts. The configuration crate passes that same value into the closure that updates Claude Code. Each contract method delegates to the corresponding operation on the CLI provider from kkt-llm-agent-command, forwards its arguments and translates the returned error into the configuration contract’s error type. The tests reject an adapter method that also calls a second CLI operation.

These are static checks of the selected routes. Running them compiles the subject and inspects the compiler output without invoking Claude Code. The identity, stacked adapter and configuration-choice rows remain separate questions.

Run `./proof/run /path/to/pinned/kashkeshet` from the mapper checkout on Linux. Cargo needs access to the locked dependencies, the pinned compiler and a native build toolchain. The runner builds in a temporary directory, prints the source locations supporting the assertions, then removes its build products.

The proof queries selected operations in the compiler’s typed intermediate representation before optimization. Each question names its endpoints. The checks recover the actual operands, follow unique local definitions, map a field read to the corresponding aggregate field, and resolve the static reference. Unknown or ambiguous shapes fail the test. This small experiment depends on the pinned compiler’s unstable text format. It does not read the architecture map.
