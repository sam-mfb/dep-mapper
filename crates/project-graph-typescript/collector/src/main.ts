// Reads the TypeScript project at --root with the tsconfig named by --tsconfig
// and prints its source evidence as JSON Lines (see records.ts). Errors that
// stop collection go to standard error with exit code 2.

import path from "node:path";
import { parseArgs } from "node:util";
import { collectFacts } from "./facts.ts";
import { collectItems } from "./items.ts";
import { Project } from "./project.ts";
import type { Record } from "./records.ts";
import { openTypeScript6, TsConfigError } from "./typescript6.ts";
import { readWorkspace, WorkspaceError } from "./workspace.ts";

function run(): void {
  const { values } = parseArgs({
    options: { root: { type: "string" }, tsconfig: { type: "string" } },
    strict: true,
  });
  if (!values.root || !values.tsconfig) {
    throw new UsageError("usage: main.ts --root <project directory> --tsconfig <path relative to root>");
  }
  const root = path.resolve(values.root);
  const workspace = readWorkspace(root);
  const checker = openTypeScript6(path.join(root, values.tsconfig));
  const project = new Project(checker, workspace);
  const emit = (record: Record) => process.stdout.write(JSON.stringify(record) + "\n");

  emit({ record: "project", name: workspace.name, settings: checker.settings() });
  for (const pkg of workspace.packages) {
    emit({ record: "module", id: pkg.name, name: pkg.name, root: pkg.root });
  }
  collectItems(project, emit);
  collectFacts(project, emit);
  emit({ record: "end" });
}

class UsageError extends Error {}

try {
  run();
} catch (error) {
  if (error instanceof UsageError || error instanceof WorkspaceError || error instanceof TsConfigError) {
    process.stderr.write(`${error.message}\n`);
    process.exit(2);
  }
  throw error;
}
