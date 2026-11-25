// The TypeScript checker calls the collector depends on. Everything that
// touches a particular TypeScript compiler API goes through this interface,
// so moving from the TypeScript 6 API to the TypeScript 7 API replaces only
// its implementation (typescript6.ts).
//
// Syntax nodes, symbols and types are passed through as the compiler gives
// them. The collector reads syntax with the `typescript` package's node
// predicates, which the TypeScript 7 API keeps.

import type ts from "typescript";

export interface Diagnostic {
  file: ts.SourceFile | undefined;
  start: number | undefined;
  length: number | undefined;
  message: string;
}

export interface Checker {
  /** The source files the selected tsconfig includes, without declaration files. */
  projectFiles(): readonly ts.SourceFile[];
  /** Whether the file is one of the compiler's own library files. */
  isDefaultLibrary(file: ts.SourceFile): boolean;
  /** The symbol a name refers to, before following import aliases. */
  symbolAt(node: ts.Node): ts.Symbol | undefined;
  /** The symbol an import or export alias finally names, or the symbol itself. */
  resolveAlias(symbol: ts.Symbol): ts.Symbol;
  /** The declaration of the signature a call or `new` expression resolves to. */
  calledDeclaration(call: ts.CallExpression | ts.NewExpression): ts.Declaration | undefined;
  /** The static type of an expression or declaration name. */
  typeAt(node: ts.Node): ts.Type;
  /** The type of the slot an expression is written into, when the code declares one. */
  slotType(expression: ts.Expression): ts.Type | undefined;
  /** The module symbols a source file exports, aliases resolved. */
  exportsOf(file: ts.SourceFile): ts.Symbol[];
  /** Errors the compiler reports for the file: syntax, binding and type errors. */
  errors(file: ts.SourceFile): Diagnostic[];
  /** The selected compiler options, as text, for the build unit's settings. */
  settings(): Record<string, string>;
}
