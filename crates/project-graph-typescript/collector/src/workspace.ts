// The project's npm workspace: which package each file belongs to, and what
// each package declares in its package.json.

import fs from "node:fs";
import path from "node:path";

export interface Dependency {
  name: string;
  /** Byte range of the dependency's key in package.json. */
  start: number;
  end: number;
}

export interface Package {
  /** The package name, which is also its module ID. */
  name: string;
  /** Directory relative to the project root, with forward slashes. */
  root: string;
  /** package.json relative to the project root. */
  manifest: string;
  /**
   * The package's source entry file, relative to the project root: the
   * `source` field; else `types` or `main` when it names a TypeScript source
   * file; else the first of `src/index.ts`, `src/index.tsx`, `index.ts` that
   * exists. Its exports are the package's surface, and an import of the
   * package that the compiler cannot resolve, such as one whose built output
   * does not exist yet, resolves to it.
   */
  entry: string | null;
  dependencies: Dependency[];
}

export interface Workspace {
  /** The root package's name. */
  name: string;
  packages: Package[];
  /** The workspace package that holds a file, if any. */
  packageOf(file: string): Package | undefined;
  /** The name of the package outside the workspace that holds a file. */
  externalNameOf(file: string): string;
  /** A path relative to the project root, with forward slashes. */
  relative(file: string): string;
}

export class WorkspaceError extends Error {}

const SECTIONS = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"];

export function readWorkspace(root: string): Workspace {
  const rootManifest = readManifest(path.join(root, "package.json"));
  const name = stringField(rootManifest.json, "name") ?? path.basename(root);
  const globs = rootManifest.json.workspaces;
  if (!Array.isArray(globs)) {
    throw new WorkspaceError(`${path.join(root, "package.json")} lists no workspaces`);
  }
  const relative = (file: string) => path.relative(root, file).split(path.sep).join("/");
  const packages = globs
    .flatMap((glob: unknown) => expand(root, String(glob)))
    .map((dir) => readPackage(root, dir, relative));
  const byDepth = [...packages].sort((a, b) => b.root.length - a.root.length);

  return {
    name,
    packages,
    relative,
    packageOf: (file) => {
      const rel = relative(path.resolve(file));
      return byDepth.find((p) => rel.startsWith(p.root + "/"));
    },
    externalNameOf: (file) => {
      let dir = path.dirname(path.resolve(file));
      while (dir.startsWith(root) || root.startsWith(dir)) {
        const manifest = path.join(dir, "package.json");
        if (fs.existsSync(manifest)) {
          const found = stringField(readManifest(manifest).json, "name");
          if (found) return found;
        }
        const parent = path.dirname(dir);
        if (parent === dir) break;
        dir = parent;
      }
      return relative(path.resolve(file));
    },
  };
}

/** Supports a literal directory or a directory followed by a single `*`. */
function expand(root: string, glob: string): string[] {
  if (glob.endsWith("/*")) {
    const base = path.join(root, glob.slice(0, -2));
    if (!fs.existsSync(base)) return [];
    return fs
      .readdirSync(base, { withFileTypes: true })
      .filter((e) => e.isDirectory() && fs.existsSync(path.join(base, e.name, "package.json")))
      .map((e) => path.join(base, e.name))
      .sort();
  }
  if (glob.includes("*")) {
    throw new WorkspaceError(`unsupported workspace pattern ${glob}`);
  }
  return [path.join(root, glob)];
}

function readPackage(root: string, dir: string, relative: (f: string) => string): Package {
  const manifestPath = path.join(dir, "package.json");
  const { json, text } = readManifest(manifestPath);
  const name = stringField(json, "name");
  if (!name) throw new WorkspaceError(`${manifestPath} has no name`);
  const entryFile = sourceEntry(dir, json);
  const dependencies: Dependency[] = [];
  for (const section of SECTIONS) {
    const listed = json[section];
    if (!listed || typeof listed !== "object") continue;
    const sectionAt = text.indexOf(`"${section}"`);
    for (const dep of Object.keys(listed)) {
      const key = `"${dep}"`;
      const at = text.indexOf(key, sectionAt);
      const start = Buffer.byteLength(text.slice(0, at));
      dependencies.push({ name: dep, start, end: start + Buffer.byteLength(key) });
    }
  }
  return {
    name,
    root: relative(dir),
    manifest: relative(manifestPath),
    entry: entryFile ? relative(entryFile) : null,
    dependencies,
  };
}

const SOURCE = /\.(ts|tsx|mts|cts)$/;
const DECLARATION = /\.d\.(ts|mts|cts)$/;
const CONVENTIONAL = ["src/index.ts", "src/index.tsx", "index.ts"];

function sourceEntry(dir: string, json: Record<string, unknown>): string | undefined {
  const named = [stringField(json, "source"), stringField(json, "types"), stringField(json, "main")];
  for (const field of named) {
    if (!field || !SOURCE.test(field) || DECLARATION.test(field)) continue;
    const file = path.join(dir, field);
    if (fs.existsSync(file)) return file;
  }
  return CONVENTIONAL.map((f) => path.join(dir, f)).find((f) => fs.existsSync(f));
}

function readManifest(file: string): { json: Record<string, unknown>; text: string } {
  if (!fs.existsSync(file)) throw new WorkspaceError(`${file} not found`);
  const text = fs.readFileSync(file, "utf8");
  try {
    return { json: JSON.parse(text), text };
  } catch (error) {
    throw new WorkspaceError(`${file} is not valid JSON: ${(error as Error).message}`);
  }
}

function stringField(json: Record<string, unknown>, field: string): string | undefined {
  const value = json[field];
  return typeof value === "string" ? value : undefined;
}
