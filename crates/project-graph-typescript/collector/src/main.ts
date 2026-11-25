// Reads the TypeScript project at --root with the tsconfig named by --tsconfig
// and prints its source evidence as JSON Lines (see records.ts). Errors that
// stop collection go to standard error with exit code 2.

import { createHash } from "node:crypto";
import fs from "node:fs";
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
  const entries = new Map(
    workspace.packages.flatMap((p) => (p.entry ? [[p.name, path.join(root, p.entry)] as const] : [])),
  );
  const checker = openTypeScript6(path.join(root, values.tsconfig), entries);
  const project = new Project(checker, workspace);
  const emit = (record: Record) => process.stdout.write(JSON.stringify(record) + "\n");

  emit({
    record: "project",
    name: workspace.name,
    settings: checker.settings(),
    sources: sourcesHash(root, [
      path.join(root, "package.json"),
      ...workspace.packages.map((p) => path.join(root, p.manifest)),
      ...checker.consultedFiles(),
    ]),
  });
  for (const pkg of workspace.packages) {
    emit({ record: "module", id: pkg.name, name: pkg.name, root: pkg.root });
  }
  collectItems(project, emit);
  collectFacts(project, emit);
  emit({ record: "end" });
}

/** Hashes each file's path relative to `root` and its contents, in path order. */
function sourcesHash(root: string, files: string[]): string {
  const hash = createHash("sha256");
  const relative = [...new Set(files.map((f) => path.relative(root, path.resolve(f)).split(path.sep).join("/")))].sort();
  for (const file of relative) {
    const contents = fs.readFileSync(path.join(root, file));
    hash.update(`${file}\0${contents.length}\0`);
    hash.update(contents);
  }
  return hash.digest("hex");
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
