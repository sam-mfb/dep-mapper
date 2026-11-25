// What the item and fact passes share: identifiers, spans, and the mapping
// from declarations to items or external names.

import ts from "typescript";
import type { Checker } from "./checker.ts";
import type { Endpoint, Span } from "./records.ts";
import type { Workspace } from "./workspace.ts";

export const LIBRARY = "lib";

export class Project {
  /** Item IDs by declaration node, filled by the item pass. */
  readonly items = new Map<ts.Node, string>();
  /** Item IDs of each source file's own module item. */
  readonly fileItems = new Map<ts.SourceFile, string>();
  /** The module of each item, by item ID. */
  readonly itemModules = new Map<string, string>();
  /** IDs of type items: classes, interfaces, type aliases and enums. */
  readonly typeItems = new Set<string>();
  /** Project classes that extend each class declaration. */
  readonly subclasses = new Map<ts.Node, ts.ClassLikeDeclaration[]>();
  readonly checker: Checker;
  readonly workspace: Workspace;

  constructor(checker: Checker, workspace: Workspace) {
    this.checker = checker;
    this.workspace = workspace;
  }

  /** The workspace package (module ID) a node's file belongs to. */
  moduleOf(node: ts.Node): string | undefined {
    const file = node.getSourceFile();
    if (this.checker.isDefaultLibrary(file)) return undefined;
    return this.workspace.packageOf(file.fileName)?.name;
  }

  /** The ID for a file's module item: the package name and path within it. */
  fileItemId(file: ts.SourceFile): string {
    const pkg = this.workspace.packageOf(file.fileName);
    const rel = this.workspace.relative(file.fileName);
    return pkg ? `${pkg.name}/${rel.slice(pkg.root.length + 1)}` : rel;
  }

  span(node: ts.Node): Span {
    const file = node.getSourceFile();
    return this.spanAt(file, node.getStart(file), node.getEnd());
  }

  /** The span of a range of UTF-16 positions in a file. */
  spanAt(file: ts.SourceFile, start: number, end: number): Span {
    return {
      file: this.workspace.relative(file.fileName),
      start: byteOffset(file, start),
      end: byteOffset(file, end),
    };
  }

  /** Whether an endpoint is a field: a place owned by a type. */
  isField(endpoint: Endpoint): boolean {
    return "place" in endpoint && this.typeItems.has(endpoint.place.owner) && endpoint.place.projection !== "this";
  }

  /** The item or external name a declaration stands for, if it has one. */
  endpointOf(declaration: ts.Node): Endpoint | undefined {
    const item = this.items.get(declaration);
    if (item) return { item };
    if (ts.isSourceFile(declaration)) {
      const fileItem = this.fileItems.get(declaration);
      if (fileItem) return { item: fileItem };
    }
    const file = declaration.getSourceFile();
    if (this.workspace.packageOf(file.fileName) && !this.checker.isDefaultLibrary(file)) {
      return undefined;
    }
    return { external: this.externalName(declaration) };
  }

  /** `package#Qualified.name` for a declaration outside the workspace. */
  externalName(declaration: ts.Node): string {
    const file = declaration.getSourceFile();
    const owner = this.checker.isDefaultLibrary(file)
      ? LIBRARY
      : this.workspace.externalNameOf(file.fileName);
    const name = qualifiedName(declaration);
    return name ? `${owner}#${name}` : owner;
  }

  /** The module that holds an endpoint, or undefined for one outside the workspace. */
  moduleOfEndpoint(endpoint: Endpoint): string | undefined {
    if ("module" in endpoint) return endpoint.module;
    if ("item" in endpoint) return this.itemModules.get(endpoint.item);
    if ("place" in endpoint) return this.itemModules.get(endpoint.place.owner);
    return undefined;
  }
}

/** The declared name, prefixed by its class or interface for members. */
export function qualifiedName(declaration: ts.Node): string | undefined {
  const own = declaredName(declaration);
  if (own === undefined) return undefined;
  const parent = declaration.parent;
  if (
    parent &&
    (ts.isClassLike(parent) || ts.isInterfaceDeclaration(parent) || ts.isTypeLiteralNode(parent))
  ) {
    const outer = declaredName(parent);
    return outer ? `${outer}.${own}` : own;
  }
  return own;
}

function declaredName(node: ts.Node): string | undefined {
  if (ts.isConstructorDeclaration(node)) return "constructor";
  const name = (node as ts.NamedDeclaration).name;
  if (name && (ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || ts.isStringLiteral(name))) {
    return name.text;
  }
  return undefined;
}

const offsets = new WeakMap<ts.SourceFile, boolean>();

/** Converts a UTF-16 position in the file's text to a UTF-8 byte offset. */
export function byteOffset(file: ts.SourceFile, position: number): number {
  let ascii = offsets.get(file);
  if (ascii === undefined) {
    ascii = /^[\x00-\x7f]*$/.test(file.text);
    offsets.set(file, ascii);
  }
  return ascii ? position : Buffer.byteLength(file.text.slice(0, position));
}

export function hasModifier(node: ts.Node, kind: ts.SyntaxKind): boolean {
  return (
    ts.canHaveModifiers(node) && (ts.getModifiers(node) ?? []).some((m) => m.kind === kind)
  );
}
