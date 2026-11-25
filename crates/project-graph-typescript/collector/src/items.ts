// The item pass: each project file's declared types, values, functions and
// methods, with their visibility from the package's entry file.

import ts from "typescript";
import { hasModifier, type Project } from "./project.ts";
import type { Emit, ItemKind, Visibility } from "./records.ts";
import type { Package } from "./workspace.ts";

interface Found {
  nodes: ts.Node[];
  id: string;
  module: string;
  name: string;
  kind: ItemKind;
  /** The class or interface that holds a member. */
  parent: string | null;
  isPrivate: boolean;
}

export function collectItems(project: Project, emit: Emit): void {
  const found = new Map<string, Found>();
  const files = project.checker.projectFiles();

  for (const file of files) {
    const pkg = project.workspace.packageOf(file.fileName);
    if (!pkg) {
      // A file reached by an import from outside the workspace is external.
      if (!project.checker.isSelected(file)) continue;
      emit({
        record: "gap",
        module: null,
        site: null,
        reason: `${project.workspace.relative(file.fileName)} belongs to no workspace package`,
      });
      continue;
    }
    const fileId = project.fileItemId(file);
    project.fileItems.set(file, fileId);
    const add = (node: ts.Node, name: string, kind: ItemKind, parent: Found | null) => {
      const id = parent ? `${parent.id}.${name}` : `${fileId}#${name}`;
      const existing = found.get(id);
      if (existing) {
        existing.nodes.push(node);
      } else {
        found.set(id, {
          nodes: [node],
          id,
          module: pkg.name,
          name: parent ? `${parent.name}.${name}` : name,
          kind,
          parent: parent?.id ?? null,
          // A namespace member is visible outside the namespace only when it says `export`.
          isPrivate:
            parent?.kind === "module"
              ? !exportedFromNamespace(node)
              : hasModifier(node, ts.SyntaxKind.PrivateKeyword) ||
                hasModifier(node, ts.SyntaxKind.ProtectedKeyword) ||
                ((node as ts.NamedDeclaration).name !== undefined &&
                  ts.isPrivateIdentifier((node as ts.NamedDeclaration).name!)),
        });
      }
      project.items.set(node, id);
      project.itemModules.set(id, pkg.name);
      if (kind === "type") project.typeItems.add(id);
      return found.get(id)!;
    };

    const declare = (statements: readonly ts.Statement[], scope: Found | null): void => {
      for (const statement of statements) {
        if (ts.isClassDeclaration(statement)) {
          const owner = add(statement, statement.name?.text ?? "default", "type", scope);
          for (const member of statement.members) {
            if (ts.isConstructorDeclaration(member)) {
              add(member, "constructor", "method", owner);
            } else if (
              (ts.isMethodDeclaration(member) || ts.isGetAccessor(member) || ts.isSetAccessor(member)) &&
              (ts.isIdentifier(member.name) || ts.isPrivateIdentifier(member.name))
            ) {
              add(member, member.name.text, "method", owner);
            }
          }
          recordSubclass(project, statement);
        } else if (ts.isInterfaceDeclaration(statement)) {
          const owner = add(statement, statement.name.text, "type", scope);
          for (const member of statement.members) {
            if (ts.isMethodSignature(member) && ts.isIdentifier(member.name)) {
              add(member, member.name.text, "method", owner);
            }
          }
        } else if (ts.isModuleDeclaration(statement) && ts.isIdentifier(statement.name)) {
          // A namespace; `namespace A.B {}` nests B in A.
          const namespace = add(statement, statement.name.text, "module", scope);
          const inner = statement.body;
          if (inner && ts.isModuleBlock(inner)) declare(inner.statements, namespace);
          else if (inner && ts.isModuleDeclaration(inner)) declare([inner], namespace);
        } else if (ts.isTypeAliasDeclaration(statement) || ts.isEnumDeclaration(statement)) {
          add(statement, statement.name.text, "type", scope);
        } else if (ts.isFunctionDeclaration(statement)) {
          add(statement, statement.name?.text ?? "default", "function", scope);
        } else if (ts.isVariableStatement(statement)) {
          for (const declaration of statement.declarationList.declarations) {
            if (!ts.isIdentifier(declaration.name)) continue;
            const init = declaration.initializer;
            const isFunction = init !== undefined && (ts.isArrowFunction(init) || ts.isFunctionExpression(init));
            const item = add(declaration, declaration.name.text, isFunction ? "function" : "value", scope);
            if (isFunction) {
              project.items.set(init, item.id);
            }
          }
        }
      }
    };
    declare(file.statements, null);
  }

  const exported = exportedItems(project);
  const packages = new Map(project.workspace.packages.map((p) => [p.name, p]));
  const visibility = (item: Found): Visibility => {
    if (item.parent) {
      const parent = visibility(found.get(item.parent)!);
      return parent === "exported" && item.isPrivate ? "internal" : parent;
    }
    if (!packages.get(item.module)?.entry) return "unknown";
    return exported.has(item.id) ? "exported" : "internal";
  };

  for (const file of files) {
    const id = project.fileItems.get(file);
    if (!id) continue;
    const pkg = project.workspace.packageOf(file.fileName)!;
    emit({
      record: "item",
      id,
      module: pkg.name,
      name: id.slice(pkg.name.length + 1),
      kind: "module",
      parent: null,
      visibility: fileVisibility(project, pkg, file),
      signature: null,
      declaration: project.span(file),
    });
  }
  for (const item of found.values()) {
    const declaration = item.nodes.find(hasBody) ?? item.nodes[0];
    emit({
      record: "item",
      id: item.id,
      module: item.module,
      name: item.name,
      kind: item.kind,
      parent: item.parent,
      visibility: visibility(item),
      signature: signature(declaration),
      declaration: project.span(declaration),
    });
  }
}

/**
 * Whether a declaration inside a namespace is marked `export`. The inner
 * names of a dotted namespace, `B` in `namespace A.B {}`, are always exported.
 */
function exportedFromNamespace(node: ts.Node): boolean {
  if (ts.isModuleDeclaration(node) && ts.isModuleDeclaration(node.parent)) return true;
  const holder = ts.isVariableDeclaration(node) ? node.parent.parent : node;
  return hasModifier(holder, ts.SyntaxKind.ExportKeyword);
}

function fileVisibility(project: Project, pkg: Package, file: ts.SourceFile): Visibility {
  if (!pkg.entry) return "unknown";
  return project.workspace.relative(file.fileName) === pkg.entry ? "exported" : "internal";
}

/** Items the package entry files export, directly or through re-exports. */
function exportedItems(project: Project): Set<string> {
  const exported = new Set<string>();
  for (const file of project.checker.projectFiles()) {
    const pkg = project.workspace.packageOf(file.fileName);
    if (!pkg?.entry || project.workspace.relative(file.fileName) !== pkg.entry) continue;
    for (const symbol of project.checker.exportsOf(file)) {
      for (const declaration of symbol.declarations ?? []) {
        const id = project.items.get(declaration);
        if (id) exported.add(id);
      }
    }
  }
  return exported;
}

function recordSubclass(project: Project, declaration: ts.ClassDeclaration): void {
  for (const clause of declaration.heritageClauses ?? []) {
    if (clause.token !== ts.SyntaxKind.ExtendsKeyword) continue;
    for (const base of clause.types) {
      const symbol = project.checker.symbolAt(base.expression);
      const resolved = symbol && project.checker.resolveAlias(symbol);
      for (const baseDeclaration of resolved?.declarations ?? []) {
        if (!ts.isClassLike(baseDeclaration)) continue;
        const list = project.subclasses.get(baseDeclaration) ?? [];
        list.push(declaration);
        project.subclasses.set(baseDeclaration, list);
      }
    }
  }
}

function hasBody(node: ts.Node): boolean {
  return (ts.isFunctionLike(node) && "body" in node && node.body !== undefined) || !ts.isFunctionLike(node);
}

/** The declaration's header as written, with whitespace collapsed. */
function signature(node: ts.Node): string | null {
  const file = node.getSourceFile();
  const start = node.getStart(file);
  let end = node.getEnd();
  if (ts.isClassDeclaration(node) || ts.isInterfaceDeclaration(node) || ts.isEnumDeclaration(node)) {
    end = node.members.pos - 1;
  } else if (ts.isFunctionLike(node) && "body" in node && node.body) {
    end = node.body.getStart(file);
  } else if (ts.isVariableDeclaration(node)) {
    const keyword = node.parent.flags & ts.NodeFlags.Const ? "const" : node.parent.flags & ts.NodeFlags.Let ? "let" : "var";
    const text = file.text.slice(start, node.initializer ? node.initializer.getStart(file) : end);
    return collapse(`${keyword} ${text.replace(/=\s*$/, "")}`);
  }
  return collapse(file.text.slice(start, end));
}

function collapse(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}
