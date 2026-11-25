// The fact pass: one-hop facts between endpoints, and the value moves that
// the route tracer follows.
//
// Imports, re-exports and references are recorded only when they leave the
// file's own package; calls, implementations and value moves are recorded
// everywhere, since routes pass through code inside one package.
//
// Value moves name their slots as places: a local or parameter is
// `{ owner: function, projection: name }`, a field is
// `{ owner: class or interface, projection: name }`, a function's result is
// projection `return`, and a method call's receiver moves into the called
// method's `self`. A top-level variable is its own item.
//
// A closure (a function expression, arrow function or nested function that
// is not an item) shares its enclosing item's places, with names marked
// `name@closure:<byte offset of the closure>`. When a closure uses a variable
// declared outside it, a capture move takes the outer place into the
// closure's own place at the closure's site.
//
// A name bound by an object pattern reads the field of the pattern's type it
// names, as if written `const name = value.field`.

import ts from "typescript";
import { hasModifier, LIBRARY, type Project } from "./project.ts";
import type { Dispatch, Emit, Endpoint, FactKind, ImportUse, Span } from "./records.ts";

type MoveKind = "store" | "read" | "pass" | "return";

export function collectFacts(project: Project, emit: Emit): void {
  const facts = new FactWriter(emit);
  for (const pkg of project.workspace.packages) {
    for (const dep of pkg.dependencies) {
      const isMember = project.workspace.packages.some((p) => p.name === dep.name);
      facts.write(pkg.name, { module: pkg.name }, { kind: "declared_dependency" }, isMember ? { module: dep.name } : { external: dep.name }, {
        file: pkg.manifest,
        start: dep.start,
        end: dep.end,
      });
    }
  }
  for (const file of project.checker.projectFiles()) {
    const fileItem = project.fileItems.get(file);
    const pkg = project.workspace.packageOf(file.fileName);
    if (!fileItem || !pkg) continue;
    new FileFacts(project, facts, file, fileItem, pkg.name).collect();
    for (const error of project.checker.errors(file)) {
      emit({
        record: "gap",
        module: pkg.name,
        site:
          error.start === undefined || error.length === undefined
            ? null
            : project.spanAt(file, error.start, error.start + error.length),
        reason: `TypeScript error: ${error.message}`,
      });
    }
  }
}

class FactWriter {
  private readonly seen = new Map<string, number>();
  private readonly emit: Emit;

  constructor(emit: Emit) {
    this.emit = emit;
  }

  write(module: string, subject: Endpoint, fact: FactKind, object: Endpoint, site: Span, condition: string | null = null): string {
    const base = `${fact.kind}@${site.file}:${site.start}`;
    const count = this.seen.get(base) ?? 0;
    this.seen.set(base, count + 1);
    const id = count === 0 ? base : `${base}~${count}`;
    this.emit({ record: "fact", id, module, subject, fact, object, site, condition });
    return id;
  }

  receiver(call: string, moves: string[]): void {
    this.emit({ record: "receiver", call, moves });
  }
}

class FileFacts {
  private readonly project: Project;
  private readonly facts: FactWriter;
  private readonly file: ts.SourceFile;
  private readonly fileItem: string;
  private readonly module: string;
  private readonly identifiers: ts.Identifier[] = [];
  private readonly witnessed = new Set<string>();
  private readonly captured = new Map<string, Endpoint>();

  constructor(project: Project, facts: FactWriter, file: ts.SourceFile, fileItem: string, module: string) {
    this.project = project;
    this.facts = facts;
    this.file = file;
    this.fileItem = fileItem;
    this.module = module;
  }

  collect(): void {
    const gather = (node: ts.Node) => {
      if (ts.isIdentifier(node)) this.identifiers.push(node);
      ts.forEachChild(node, gather);
    };
    gather(this.file);
    for (const statement of this.file.statements) {
      if (ts.isImportDeclaration(statement)) this.imports(statement);
      else if (ts.isExportDeclaration(statement)) this.reexports(statement);
    }
    this.visit(this.file);
  }

  private get checker() {
    return this.project.checker;
  }

  private write(subject: Endpoint, fact: FactKind, object: Endpoint, site: ts.Node, condition: string | null = null): string {
    return this.facts.write(this.module, subject, fact, object, this.project.span(site), condition);
  }

  /** Whether an endpoint lies outside this file's package. */
  private leavesPackage(endpoint: Endpoint): boolean {
    return this.project.moduleOfEndpoint(endpoint) !== this.module;
  }

  // Imports and re-exports

  private imports(declaration: ts.ImportDeclaration): void {
    const clause = declaration.importClause;
    if (!clause) return;
    const names: { name: ts.Identifier; site: ts.Node; typeOnly: boolean }[] = [];
    if (clause.name) names.push({ name: clause.name, site: clause.name, typeOnly: clause.isTypeOnly });
    const bindings = clause.namedBindings;
    if (bindings && ts.isNamespaceImport(bindings)) {
      names.push({ name: bindings.name, site: bindings, typeOnly: clause.isTypeOnly });
    } else if (bindings) {
      for (const element of bindings.elements) {
        names.push({ name: element.name, site: element, typeOnly: clause.isTypeOnly || element.isTypeOnly });
      }
    }
    for (const { name, site, typeOnly } of names) {
      const local = this.checker.symbolAt(name);
      const target = local && this.targetOf(local);
      if (!target || !this.leavesPackage(target)) continue;
      this.write({ item: this.fileItem }, { kind: "import", local_name: name.text, usage: this.usage(local!, name, typeOnly) }, target, site);
    }
  }

  private reexports(declaration: ts.ExportDeclaration): void {
    const clause = declaration.exportClause;
    if (!clause || ts.isNamespaceExport(clause)) {
      const specifier = declaration.moduleSpecifier;
      const module = specifier && this.checker.symbolAt(specifier);
      const target = module && this.targetOf(module);
      if (!target || !this.leavesPackage(target)) return;
      const exported_name = clause ? clause.name.text : "*";
      this.write({ item: this.fileItem }, { kind: "reexport", exported_name }, target, declaration);
      return;
    }
    for (const element of clause.elements) {
      const symbol = this.checker.symbolAt(element.name);
      const target = symbol && this.targetOf(symbol);
      if (!target || !this.leavesPackage(target)) continue;
      this.write({ item: this.fileItem }, { kind: "reexport", exported_name: element.name.text }, target, element);
    }
  }

  private targetOf(symbol: ts.Symbol): Endpoint | undefined {
    const declaration = this.checker.resolveAlias(symbol).declarations?.[0];
    return declaration && this.project.endpointOf(declaration);
  }

  private usage(local: ts.Symbol, declaredAt: ts.Identifier, typeOnly: boolean): ImportUse {
    const uses = this.identifiers.filter(
      (id) => id !== declaredAt && !insideImport(id) && this.checker.symbolAt(id) === local,
    );
    if (uses.length === 0) return "unused";
    if (typeOnly || uses.every(inTypePosition)) return "type_only";
    return "value";
  }

  // Body facts

  private visit(node: ts.Node): void {
    if (ts.isIdentifier(node)) {
      this.reference(node);
    } else if (ts.isCallExpression(node)) {
      this.call(node);
    } else if (ts.isNewExpression(node)) {
      this.construction(node);
    } else if (ts.isClassDeclaration(node)) {
      this.declaredImplements(node);
    } else if (ts.isVariableDeclaration(node) && node.initializer) {
      if (ts.isArrayBindingPattern(node.name)) {
        this.arrayPattern(node.name, node.initializer);
      } else {
        const destination = this.declarationSlot(node);
        if (destination) this.flow(node.initializer, destination, "store", null);
      }
    } else if (ts.isBindingElement(node) && ts.isIdentifier(node.name)) {
      this.bindingElement(node, node.name);
    } else if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      const destination = this.targetSlot(node.left);
      if (destination) this.flow(node.right, destination, "store", null);
    } else if (ts.isReturnStatement(node) && node.expression) {
      const fn = ts.findAncestor(node.parent, ts.isFunctionLike);
      const owner = fn && this.project.items.get(fn);
      if (owner) this.flow(node.expression, { place: { owner, projection: "return" } }, "return", null);
    } else if (ts.isArrowFunction(node) && !ts.isBlock(node.body)) {
      const owner = this.project.items.get(node);
      if (owner) this.flow(node.body, { place: { owner, projection: "return" } }, "return", null);
    } else if (ts.isPropertyDeclaration(node) && node.initializer && ts.isIdentifier(node.name)) {
      const owner = this.project.items.get(node.parent);
      if (owner) this.flow(node.initializer, { place: { owner, projection: node.name.text } }, "store", null);
    } else if (ts.isParameter(node) && ts.isParameterPropertyDeclaration(node, node.parent) && ts.isIdentifier(node.name)) {
      const constructor = this.project.items.get(node.parent);
      const owner = this.project.items.get(node.parent.parent);
      if (constructor && owner) {
        const name = node.name.text;
        this.write({ place: { owner: constructor, projection: name } }, { kind: "store" }, { place: { owner, projection: name } }, node);
      }
    }
    ts.forEachChild(node, (child) => this.visit(child));
  }

  /** A type or value reference to an item in another package. */
  private reference(id: ts.Identifier): void {
    const parent = id.parent;
    if (insideImport(id) || insideExport(id)) return;
    if ((parent as ts.NamedDeclaration).name === id) return;
    if (ts.isPropertyAccessExpression(parent) && parent.name === id) return;
    if ((ts.isCallExpression(parent) || ts.isNewExpression(parent)) && parent.expression === id) return;
    if (ts.isExpressionWithTypeArguments(parent) && ts.isHeritageClause(parent.parent) && parent.parent.token === ts.SyntaxKind.ImplementsKeyword) return;
    const symbol = this.checker.symbolAt(id);
    const target = symbol && this.targetOf(symbol);
    if (!target || !this.leavesPackage(target)) return;
    if ("external" in target && target.external.startsWith(`${LIBRARY}#`)) return;
    const kind = inTypePosition(id) ? "type_reference" : "value_reference";
    this.write({ item: this.ownerOf(id) }, { kind }, target, id);
  }

  private call(call: ts.CallExpression): void {
    const declaration = this.checker.calledDeclaration(call);
    const target = declaration && this.project.endpointOf(declaration);
    const owner: Endpoint = { item: this.ownerOf(call) };
    if (!declaration || !target) {
      this.write(owner, { kind: "call", dispatch: "indirect" }, { unresolved: call.expression.getText(this.file) }, call);
      return;
    }
    const dispatch: Dispatch = "external" in target ? "foreign" : this.dispatchOf(declaration);
    const id = this.write(owner, { kind: "call", dispatch }, target, call);
    if ("item" in target && ts.isPropertyAccessExpression(call.expression) && isInstanceMethod(declaration)) {
      const self: Endpoint = { place: { owner: target.item, projection: "self" } };
      this.facts.receiver(id, this.flow(call.expression.expression, self, "pass", null));
    }
    this.arguments(call.arguments, declaration, target);
  }

  private construction(expression: ts.NewExpression): void {
    const declaration = this.checker.calledDeclaration(expression);
    const constructor = declaration && ts.isConstructorDeclaration(declaration) ? this.project.endpointOf(declaration) : undefined;
    const target = constructor ?? this.classEndpoint(expression);
    if (!target) {
      this.write({ item: this.ownerOf(expression) }, { kind: "call", dispatch: "indirect" }, { unresolved: expression.expression.getText(this.file) }, expression);
      return;
    }
    const dispatch: Dispatch = "external" in target ? "foreign" : "direct";
    this.write({ item: this.ownerOf(expression) }, { kind: "call", dispatch }, target, expression);
    if (declaration) this.arguments(expression.arguments ?? ts.factory.createNodeArray(), declaration, target);
  }

  private arguments(args: ts.NodeArray<ts.Expression>, declaration: ts.Declaration, target: Endpoint): void {
    if (!ts.isFunctionLike(declaration)) return;
    const parameters = declaration.parameters;
    args.forEach((arg, index) => {
      const last = parameters[parameters.length - 1];
      const parameter = parameters[index] ?? (last?.dotDotDotToken ? last : undefined);
      if (!parameter) return;
      if (!ts.isIdentifier(parameter.name)) {
        // A destructured parameter reads fields, so an object literal argument fills them.
        const value = skipOuter(arg);
        if (ts.isObjectLiteralExpression(value)) this.objectLiteral(value, null);
        return;
      }
      const name = parameter.name.text;
      if ("item" in target) {
        this.flow(arg, { place: { owner: target.item, projection: name } }, "pass", null);
      } else if ("external" in target && !target.external.startsWith(`${LIBRARY}#`)) {
        this.flow(arg, { external: `${target.external}(${name})` }, "pass", null);
      }
    });
  }

  private dispatchOf(declaration: ts.Declaration): Dispatch {
    if (ts.isMethodSignature(declaration)) return "contract";
    if (ts.isMethodDeclaration(declaration)) {
      if (hasModifier(declaration, ts.SyntaxKind.AbstractKeyword)) return "contract";
      if (hasModifier(declaration, ts.SyntaxKind.StaticKeyword)) return "direct";
      if (this.isOverridden(declaration)) return "contract";
    }
    return "direct";
  }

  /** Whether a project subclass declares a method of the same name. */
  private isOverridden(method: ts.MethodDeclaration): boolean {
    const name = method.name.getText(method.getSourceFile());
    const pending = [...(this.project.subclasses.get(method.parent) ?? [])];
    while (pending.length > 0) {
      const subclass = pending.pop()!;
      if (subclass.members.some((m) => ts.isMethodDeclaration(m) && m.name.getText(m.getSourceFile()) === name)) return true;
      pending.push(...(this.project.subclasses.get(subclass) ?? []));
    }
    return false;
  }

  private declaredImplements(declaration: ts.ClassDeclaration): void {
    const subject = this.project.endpointOf(declaration);
    if (!subject) return;
    for (const clause of declaration.heritageClauses ?? []) {
      if (clause.token !== ts.SyntaxKind.ImplementsKeyword) continue;
      for (const type of clause.types) {
        const symbol = this.checker.symbolAt(type.expression);
        const target = symbol && this.targetOf(symbol);
        if (target) this.write(subject, { kind: "implements" }, target, type);
      }
    }
  }

  // Value moves

  /**
   * Records the moves that put the value of `expression` into `destination`
   * and returns their fact IDs. A conditional expression moves each branch
   * under its own condition.
   */
  private flow(expression: ts.Expression, destination: Endpoint, kind: MoveKind, condition: string | null): string[] {
    const e = skipOuter(expression);
    if (ts.isConditionalExpression(e)) {
      const test = e.condition.getText(this.file);
      const negated = ts.isIdentifier(skipOuter(e.condition)) ? `!${test}` : `!(${test})`;
      return [
        ...this.flow(e.whenTrue, destination, kind, both(condition, test)),
        ...this.flow(e.whenFalse, destination, kind, both(condition, negated)),
      ];
    }
    if (ts.isObjectLiteralExpression(e)) {
      return this.objectLiteral(e, condition);
    }
    if (ts.isNewExpression(e)) {
      const origin = this.classEndpoint(e);
      if (!origin) return [];
      this.witnessImplements(e);
      return [this.write(origin, { kind: "construct" }, destination, e, condition)];
    }
    const source = this.sourceOf(e);
    if (!source) return [];
    let move: FactKind["kind"] = kind;
    if (ts.isCallExpression(e)) move = "return";
    else if (kind === "store" && this.project.isField(source) && !this.project.isField(destination)) move = "read";
    if (this.witnessImplements(e)) move = "coerce";
    return [this.write(source, { kind: move } as FactKind, destination, e, condition)];
  }

  private objectLiteral(literal: ts.ObjectLiteralExpression, condition: string | null): string[] {
    const slot = this.checker.slotType(literal);
    const declaration = slot?.getSymbol()?.declarations?.[0];
    const owner = declaration && this.project.items.get(declaration);
    if (!owner) return [];
    return literal.properties.flatMap((property) => {
      if (ts.isPropertyAssignment(property) && ts.isIdentifier(property.name)) {
        return this.flow(property.initializer, { place: { owner, projection: property.name.text } }, "store", condition);
      }
      if (ts.isShorthandPropertyAssignment(property)) {
        return this.flow(property.name, { place: { owner, projection: property.name.text } }, "store", condition);
      }
      return [];
    });
  }

  /** The endpoint a value is read from. */
  private sourceOf(e: ts.Expression): Endpoint | undefined {
    if (ts.isIdentifier(e)) return this.variableSlot(e);
    if (e.kind === ts.SyntaxKind.ThisKeyword) {
      const owner = this.ownerOf(e);
      const cls = ts.findAncestor(e, ts.isClassLike);
      const item = cls && this.project.items.get(cls);
      return item ? { place: { owner: item, projection: "this" } } : { place: { owner, projection: "this" } };
    }
    if (ts.isPropertyAccessExpression(e)) return this.fieldSlot(e);
    if (ts.isCallExpression(e)) {
      const declaration = this.checker.calledDeclaration(e);
      const target = declaration && this.project.endpointOf(declaration);
      if (!target) return undefined;
      if ("item" in target) return { place: { owner: target.item, projection: "return" } };
      return target;
    }
    return undefined;
  }

  /**
   * When a class instance enters a slot typed by an interface or abstract
   * class that the class does not declare, records the structural
   * implementation it witnesses. Returns whether the move is such a coercion.
   */
  private witnessImplements(e: ts.Expression): boolean {
    const slot = this.checker.slotType(e);
    if (!slot) return false;
    const cls = this.checker.typeAt(e).getSymbol()?.declarations?.find(ts.isClassLike);
    const contract = slot.getSymbol()?.declarations?.find(
      (d) => ts.isInterfaceDeclaration(d) || (ts.isClassLike(d) && hasModifier(d, ts.SyntaxKind.AbstractKeyword)),
    );
    if (!cls || !contract || cls === contract) return false;
    const subject = this.project.endpointOf(cls);
    const object = this.project.endpointOf(contract);
    if (!subject || !object) return true;
    const key = `${JSON.stringify(subject)} ${JSON.stringify(object)}`;
    if (!declares(this.project, cls, contract) && !this.witnessed.has(key)) {
      this.witnessed.add(key);
      this.write(subject, { kind: "implements" }, object, e);
    }
    return true;
  }

  private classEndpoint(expression: ts.NewExpression): Endpoint | undefined {
    const symbol = this.checker.symbolAt(expression.expression);
    const cls = symbol && this.checker.resolveAlias(symbol).declarations?.find(ts.isClassLike);
    return cls && this.project.endpointOf(cls);
  }

  // Slots

  private declarationSlot(declaration: ts.VariableDeclaration): Endpoint | undefined {
    const item = this.project.items.get(declaration);
    if (item) return { item };
    if (!ts.isIdentifier(declaration.name)) return undefined;
    return this.placeOf(declaration, declaration.name.text);
  }

  private targetSlot(target: ts.Expression): Endpoint | undefined {
    const e = skipOuter(target);
    if (ts.isIdentifier(e)) return this.variableSlot(e);
    if (ts.isPropertyAccessExpression(e)) return this.fieldSlot(e);
    return undefined;
  }

  /** The slot a variable or parameter name refers to, as seen from where the name is used. */
  private variableSlot(id: ts.Identifier): Endpoint | undefined {
    const symbol = this.checker.symbolAt(id);
    if (!symbol) return undefined;
    const resolved = this.checker.resolveAlias(symbol);
    const declaration = resolved.valueDeclaration ?? resolved.declarations?.[0];
    if (!declaration) return undefined;
    if (ts.isVariableDeclaration(declaration)) {
      const target = this.project.endpointOf(declaration);
      if (target) return target;
    }
    const isLocal =
      ts.isVariableDeclaration(declaration) || ts.isParameter(declaration) || ts.isBindingElement(declaration);
    const name = (declaration as ts.NamedDeclaration).name;
    if (!isLocal || !name || !ts.isIdentifier(name)) return undefined;
    if (declaration.getSourceFile() !== this.file) return undefined;
    return this.captureInto(declaration, this.placeOf(declaration, name.text), name.text, id);
  }

  /** The place of a local, parameter or bound name where it is declared. */
  private placeOf(declaration: ts.Node, name: string): Endpoint {
    if (ts.isParameter(declaration)) {
      const owner = this.project.items.get(declaration.parent);
      if (owner) return { place: { owner, projection: name } };
    }
    const closure = this.closureAround(declaration);
    const projection = closure ? `${name}@closure:${this.offset(closure)}` : name;
    return { place: { owner: this.ownerOf(declaration), projection } };
  }

  /**
   * Follows a variable from its declaration into each closure between it
   * and `use`, recording one capture per closure and variable.
   */
  private captureInto(declaration: ts.Node, place: Endpoint, name: string, use: ts.Node): Endpoint {
    const closures: ts.Node[] = [];
    for (let node = use.parent; node; node = node.parent) {
      if (this.isItemBoundary(node)) break;
      if (!isClosure(node, this.project)) continue;
      if (declaration.pos >= node.pos && declaration.end <= node.end) break;
      closures.push(node);
    }
    let current = place;
    for (const closure of closures.reverse()) {
      const key = `${this.offset(closure)} ${name} ${JSON.stringify(place)}`;
      let inner = this.captured.get(key);
      if (!inner) {
        if (!("place" in current)) return current;
        inner = { place: { owner: current.place.owner, projection: `${name}@closure:${this.offset(closure)}` } };
        this.write(current, { kind: "capture" }, inner, closure);
        this.captured.set(key, inner);
      }
      current = inner;
    }
    return current;
  }

  /** The innermost closure that holds a node, if no item lies between them. */
  private closureAround(node: ts.Node): ts.Node | undefined {
    for (let current = node.parent; current; current = current.parent) {
      if (this.isItemBoundary(current)) return undefined;
      if (isClosure(current, this.project)) return current;
    }
    return undefined;
  }

  private isItemBoundary(node: ts.Node): boolean {
    return (
      this.project.items.has(node) &&
      (ts.isFunctionLike(node) || ts.isClassLike(node) || ts.isVariableDeclaration(node))
    );
  }

  private offset(node: ts.Node): number {
    return this.project.span(node).start;
  }

  /** `name` bound by an object pattern reads the field it names; an array pattern element is handled with its literal. */
  private bindingElement(element: ts.BindingElement, name: ts.Identifier): void {
    const destination = this.placeOf(element, name.text);
    if (element.initializer) this.flow(element.initializer, destination, "store", null);
    if (!ts.isObjectBindingPattern(element.parent)) return;
    const property = element.propertyName
      ? ts.isIdentifier(element.propertyName) || ts.isStringLiteral(element.propertyName)
        ? element.propertyName.text
        : undefined
      : name.text;
    if (property === undefined) return;
    const symbol = this.checker.propertyOf(this.checker.typeAt(element.parent), property);
    const source = symbol?.valueDeclaration && this.fieldOf(symbol.valueDeclaration, property);
    if (source) this.write(source, { kind: "read" }, destination, element);
  }

  /** `const [a, b] = [x, y]` moves each element of the literal into its name. */
  private arrayPattern(pattern: ts.ArrayBindingPattern, initializer: ts.Expression): void {
    const literal = skipOuter(initializer);
    if (!ts.isArrayLiteralExpression(literal)) return;
    pattern.elements.forEach((element, index) => {
      const value = literal.elements[index];
      if (!value || ts.isOmittedExpression(element) || !ts.isIdentifier(element.name)) return;
      this.flow(value, this.placeOf(element, element.name.text), "store", null);
    });
  }

  /** The slot a property access reads or writes, when the property is a field. */
  private fieldSlot(access: ts.PropertyAccessExpression): Endpoint | undefined {
    const declaration = this.checker.symbolAt(access.name)?.valueDeclaration;
    return declaration && this.fieldOf(declaration, access.name.text);
  }

  /** The place of a field, given the property's declaration. */
  private fieldOf(declaration: ts.Declaration, name: string): Endpoint | undefined {
    let container: ts.Node | undefined;
    if (ts.isPropertyDeclaration(declaration) || ts.isPropertySignature(declaration)) {
      container = declaration.parent;
    } else if (ts.isParameter(declaration) && ts.isParameterPropertyDeclaration(declaration, declaration.parent)) {
      container = declaration.parent.parent;
    }
    if (!container) return undefined;
    const owner = this.project.items.get(container);
    if (owner) return { place: { owner, projection: name } };
    const target = this.project.endpointOf(container);
    return target && "external" in target ? { external: this.project.externalName(declaration) } : undefined;
  }

  /** The item whose code holds the node: a function, method, type, top-level variable or the file. */
  private ownerOf(node: ts.Node): string {
    for (let current = node.parent; current; current = current.parent) {
      const item = this.project.items.get(current);
      if (
        item &&
        (ts.isFunctionLike(current) ||
          ts.isClassLike(current) ||
          ts.isVariableDeclaration(current) ||
          ts.isInterfaceDeclaration(current) ||
          ts.isTypeAliasDeclaration(current) ||
          ts.isEnumDeclaration(current))
      ) {
        return item;
      }
    }
    return this.fileItem;
  }
}

/** A function that is not an item: a function expression, arrow function or nested function. */
function isClosure(node: ts.Node, project: Project): boolean {
  return (
    (ts.isArrowFunction(node) || ts.isFunctionExpression(node) || ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node)) &&
    !project.items.has(node)
  );
}

function both(outer: string | null, inner: string): string {
  return outer ? `${outer} && ${inner}` : inner;
}

function skipOuter(e: ts.Expression): ts.Expression {
  while (
    ts.isParenthesizedExpression(e) ||
    ts.isAsExpression(e) ||
    ts.isNonNullExpression(e) ||
    ts.isSatisfiesExpression(e) ||
    ts.isTypeAssertionExpression(e)
  ) {
    e = e.expression;
  }
  return e;
}

function isInstanceMethod(declaration: ts.Declaration): boolean {
  return (
    ts.isMethodSignature(declaration) ||
    (ts.isMethodDeclaration(declaration) && !hasModifier(declaration, ts.SyntaxKind.StaticKeyword))
  );
}

function declares(project: Project, cls: ts.ClassLikeDeclaration, contract: ts.Declaration): boolean {
  return (cls.heritageClauses ?? []).some((clause) =>
    clause.types.some((type) => {
      const symbol = project.checker.symbolAt(type.expression);
      return symbol !== undefined && (project.checker.resolveAlias(symbol).declarations ?? []).includes(contract);
    }),
  );
}

function insideImport(node: ts.Node): boolean {
  return ts.findAncestor(node, ts.isImportDeclaration) !== undefined;
}

function insideExport(node: ts.Node): boolean {
  return ts.findAncestor(node, ts.isExportDeclaration) !== undefined;
}

/** Whether a name is used as a type rather than a value. */
function inTypePosition(id: ts.Node): boolean {
  for (let node = id.parent; node; node = node.parent) {
    if (ts.isExpressionWithTypeArguments(node) && ts.isHeritageClause(node.parent)) {
      return node.parent.token === ts.SyntaxKind.ImplementsKeyword || ts.isInterfaceDeclaration(node.parent.parent);
    }
    if (ts.isTypeNode(node)) return true;
    if (ts.isStatement(node) || ts.isFunctionLike(node) || ts.isClassElement(node) || ts.isSourceFile(node)) return false;
  }
  return false;
}
