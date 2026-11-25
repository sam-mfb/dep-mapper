// The Checker contract on the TypeScript 6 JavaScript API.

import ts from "typescript";
import path from "node:path";
import type { Checker, Diagnostic } from "./checker.ts";

export class TsConfigError extends Error {}

/**
 * Opens the program the tsconfig at `configPath` describes. `packageEntries`
 * maps workspace package names to their source entry files, as absolute
 * paths: an import of exactly that name that the compiler cannot resolve,
 * because the package has not been built or installed, resolves to its entry.
 */
export function openTypeScript6(configPath: string, packageEntries: ReadonlyMap<string, string>): Checker {
  // Every file the config parser or the compiler reads, apart from the
  // TypeScript package's own files, such as its library declarations, which
  // belong to the TypeScript version and not to the project.
  const consulted = new Set<string>();
  const library = path.resolve(path.dirname(ts.getDefaultLibFilePath({})), "..");
  const track = (file: string, text: string | undefined): string | undefined => {
    const absolute = path.resolve(file);
    if (text !== undefined && !absolute.startsWith(library + path.sep)) consulted.add(absolute);
    return text;
  };
  const configHost: ts.ParseConfigHost = {
    useCaseSensitiveFileNames: ts.sys.useCaseSensitiveFileNames,
    readDirectory: ts.sys.readDirectory,
    fileExists: ts.sys.fileExists,
    readFile: (file) => track(file, ts.sys.readFile(file)),
  };
  const read = ts.readConfigFile(configPath, configHost.readFile);
  if (read.error) {
    throw new TsConfigError(flatten(read.error.messageText));
  }
  const parsed = ts.parseJsonConfigFileContent(read.config, configHost, path.dirname(configPath));
  if (parsed.errors.length > 0) {
    throw new TsConfigError(parsed.errors.map((e) => flatten(e.messageText)).join("; "));
  }
  const host = ts.createCompilerHost(parsed.options, true);
  const readFile = host.readFile.bind(host);
  host.readFile = (file) => track(file, readFile(file));
  const cache = ts.createModuleResolutionCache(host.getCurrentDirectory(), host.getCanonicalFileName, parsed.options);
  host.resolveModuleNameLiterals = (literals, containingFile, redirected, options, containingSourceFile) =>
    literals.map((literal) => {
      const mode = ts.getModeForUsageLocation(containingSourceFile, literal, options);
      const resolved = ts.resolveModuleName(literal.text, containingFile, options, host, cache, redirected, mode);
      const entry = packageEntries.get(literal.text);
      if (resolved.resolvedModule || !entry) return resolved;
      return {
        resolvedModule: {
          resolvedFileName: entry,
          extension: extensionOf(entry),
          isExternalLibraryImport: false,
        },
      };
    });
  const program = ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
    projectReferences: parsed.projectReferences,
    host,
  });
  const checker = program.getTypeChecker();
  const roots = new Set(parsed.fileNames.map((f) => path.resolve(f)));

  return {
    projectFiles: () =>
      program
        .getSourceFiles()
        .filter((f) => !f.isDeclarationFile && !program.isSourceFileFromExternalLibrary(f)),
    isSelected: (file) => roots.has(path.resolve(file.fileName)),
    consultedFiles: () => [...consulted].sort(),
    isDefaultLibrary: (file) => program.isSourceFileDefaultLibrary(file),
    symbolAt: (node) => checker.getSymbolAtLocation(node),
    resolveAlias: (symbol) =>
      symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol,
    calledDeclaration: (call) => checker.getResolvedSignature(call)?.declaration as ts.Declaration | undefined,
    typeAt: (node) => checker.getTypeAtLocation(node),
    propertyOf: (type, name) => checker.getPropertyOfType(type, name),
    elementType: (type) => checker.getIndexTypeOfType(type, ts.IndexKind.Number),
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

function extensionOf(file: string): ts.Extension {
  if (file.endsWith(".tsx")) return ts.Extension.Tsx;
  if (file.endsWith(".mts")) return ts.Extension.Mts;
  if (file.endsWith(".cts")) return ts.Extension.Cts;
  return ts.Extension.Ts;
}

function flatten(message: string | ts.DiagnosticMessageChain): string {
  return ts.flattenDiagnosticMessageText(message, " ");
}
