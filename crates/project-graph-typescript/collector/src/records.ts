// The records the collector prints, one JSON object per line. The Rust crate
// project-graph-typescript reads them (src/collection/records.rs); the two
// definitions must change together.

export interface Span {
  file: string;
  start: number;
  end: number;
}

export type Endpoint =
  | { module: string }
  | { item: string }
  | { place: { owner: string; projection: string } }
  | { external: string }
  | { unresolved: string };

export type ImportUse = "unused" | "type_only" | "value";
export type Dispatch = "direct" | "contract" | "indirect" | "foreign";

export type FactKind =
  | { kind: "declared_dependency" }
  | { kind: "import"; local_name: string; usage: ImportUse }
  | { kind: "reexport"; exported_name: string }
  | { kind: "type_reference" }
  | { kind: "value_reference" }
  | { kind: "implements" }
  | { kind: "call"; dispatch: Dispatch }
  | { kind: "construct" }
  | { kind: "store" }
  | { kind: "read" }
  | { kind: "pass" }
  | { kind: "return" }
  | { kind: "coerce" }
  | { kind: "capture" };

/**
 * The calls a move crosses, each named by the span of its call expression
 * (`file:start-end`): the call whose result it takes, and the call it enters
 * as an argument or receiver. `f(g())` crosses both.
 */
export type Link = { argument: string | null; returned: string | null };

export type ItemKind = "type" | "value" | "function" | "method" | "module";
export type Visibility = "exported" | "internal" | "unknown";

export type Record =
  | {
      record: "project";
      name: string;
      settings: { [key: string]: string };
      /** SHA-256, in hex, of every file the collector and the compiler read, apart from the compiler's library. */
      sources: string;
    }
  | { record: "module"; id: string; name: string; root: string }
  | {
      record: "item";
      id: string;
      module: string;
      name: string;
      kind: ItemKind;
      /** For a method, the class or interface that declares it. */
      parent: string | null;
      visibility: Visibility;
      signature: string | null;
      declaration: Span | null;
    }
  | {
      record: "fact";
      id: string;
      module: string;
      subject: Endpoint;
      fact: FactKind;
      object: Endpoint;
      site: Span | null;
      /** Set when the move happens only on one branch of a conditional. */
      condition: string | null;
      /** For a move into a callee's parameter or out of its result, the call site. */
      link: Link | null;
    }
  /** A method call whose receiver the collector followed into the callee's `self`. */
  | { record: "receiver"; call: string; moves: string[] }
  | { record: "gap"; module: string | null; site: Span | null; reason: string }
  /** The last record. Output without it is incomplete. */
  | { record: "end" };

export type Emit = (record: Record) => void;
