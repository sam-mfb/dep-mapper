// The Checker contract on the TypeScript 6 JavaScript API.

import ts from "typescript";
import path from "node:path";
import type { Checker, Diagnostic } from "./checker.ts";

export class TsConfigError extends Error {}

/** Opens the program the tsconfig at `configPath` describes. */
export function openTypeScript6(configPath: string): Checker {
  const read = ts.readConfigFile(configPath, ts.sys.readFile);
  if (read.error) {
    throw new TsConfigError(flatten(read.error.messageText));
  }
  const parsed = ts.parseJsonConfigFileContent(read.config, ts.sys, path.dirname(configPath));
  if (parsed.errors.length > 0) {
    throw new TsConfigError(parsed.errors.map((e) => flatten(e.messageText)).join("; "));
  }
  const program = ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
    projectReferences: parsed.projectReferences,
  });
  const checker = program.getTypeChecker();
  const roots = new Set(parsed.fileNames.map((f) => path.resolve(f)));

  return {
    projectFiles: () =>
      program
        .getSourceFiles()
        .filter((f) => !f.isDeclarationFile && !program.isSourceFileFromExternalLibrary(f)),
    isSelected: (file) => roots.has(path.resolve(file.fileName)),
    isDefaultLibrary: (file) => program.isSourceFileDefaultLibrary(file),
    symbolAt: (node) => checker.getSymbolAtLocation(node),
    resolveAlias: (symbol) =>
      symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol,
    calledDeclaration: (call) => checker.getResolvedSignature(call)?.declaration as ts.Declaration | undefined,
    typeAt: (node) => checker.getTypeAtLocation(node),
    slotType: (expression) => checker.getContextualType(expression),
    exportsOf: (file) => {
      const module = checker.getSymbolAtLocation(file);
      if (!module) return [];
      return checker
        .getExportsOfModule(module)
        .map((s) => (s.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(s) : s));
    },
    errors: (file) =>
      [...program.getSyntacticDiagnostics(file), ...program.getSemanticDiagnostics(file)]
        .filter((d) => d.category === ts.DiagnosticCategory.Error)
        .map(
          (d): Diagnostic => ({
            file: d.file,
            start: d.start,
            length: d.length,
            message: flatten(d.messageText),
          }),
        ),
    settings: () => {
      const o = parsed.options;
      return {
        typescript: ts.version,
        strict: String(o.strict === true),
        target: o.target === undefined ? "unset" : ts.ScriptTarget[o.target],
        module: o.module === undefined ? "unset" : ts.ModuleKind[o.module],
        moduleResolution:
          o.moduleResolution === undefined ? "unset" : ts.ModuleResolutionKind[o.moduleResolution],
      };
    },
  };
}

function flatten(message: string | ts.DiagnosticMessageChain): string {
  return ts.flattenDiagnosticMessageText(message, " ");
}
