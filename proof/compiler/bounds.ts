// Closed executable evidence. A Rust impl is a possible recipient. A table is
// build evidence for a concrete instantiation. Neither describes a value handoff.
export type Item = { id: string; crate: string; unit: string; name: string };
type Candidate = {
  impl: Item;
  nominal: Item | null;
  selfPattern: string;
  methods: { member: Item; body: Item }[];
};
type Envelope = {
  kind: 'impl-envelope';
  binary: Item;
  contract: Item;
  candidates: Candidate[];
  supers?: Item[];
};
type Table = {
  kind: 'dispatch-table';
  emitter?: string;
  contracts: Item[];
  principal: Item;
  typeId: string;
  typeName: string;
  nominal: Item | null;
};
type Graph = { kind: 'executable-graph'; binary: Item; units: string[] };
export class Bounds {
  private graphs: Graph[] = [];
  private units?: Set<string>;
  private envelopes = new Map<string, Envelope>();
  private raw: Table[] = [];
  private tables = new Map<string, Map<string, Table>>();
  add(f: Envelope | Table | Graph) {
    if (f.kind === 'executable-graph') this.graphs.push(f);
    if (f.kind === 'impl-envelope') this.envelopes.set(f.contract.id, f);
    if (f.kind === 'dispatch-table') this.raw.push(f);
  }
  finish(required: boolean) {
    if (!required && !this.graphs.length) return;
    if (this.graphs.length !== 1) throw new Error('dispatch bounds require one selected executable');
    const units = new Set(this.graphs[0].units);
    this.units = units;
    for (const t of this.raw) {
      if (!t.emitter || !units.has(t.emitter)) continue;
      for (const c of t.contracts) {
        if (!this.tables.has(c.id)) this.tables.set(c.id, new Map());
        this.tables.get(c.id)!.set(t.typeId, t);
      }
    }
  }
  includes(caller: Item) {
    return !this.units || this.units.has(caller.unit);
  }
  satisfies(source: Item | undefined, target: Item | undefined) {
    return (
      !!source &&
      !!target &&
      (source.id === target.id ||
        !!this.envelopes.get(source.id)?.supers?.some((s) => s.id === target.id))
    );
  }
  candidates(contract: Item, principal?: Item) {
    const base = this.envelopes.get(contract.id);
    const sub = principal && this.envelopes.get(principal.id);
    if (!base) return;
    if (
      !sub ||
      !this.satisfies(principal, contract) ||
      principal!.id === contract.id ||
      sub.candidates.some((c) => !c.nominal)
    )
      return base.candidates;
    const allowed = new Set(sub.candidates.map((c) => c.nominal!.id));
    return base.candidates.filter((c) => !c.nominal || allowed.has(c.nominal.id));
  }
  singleBody(contract: Item, member: Item, principal?: Item): Item | undefined {
    const cs = this.candidates(contract, principal);
    if (cs?.length !== 1) return;
    return cs[0].methods.find((m) => m.member.id === member.id)?.body;
  }
  describe(contract: Item, member: Item, principal?: Item) {
    const cs = this.candidates(contract, principal);
    if (!cs) return;
    const instances = [
      ...(this.tables.get(this.satisfies(principal, contract) ? principal!.id : contract.id)?.values() ??
        []),
    ];
    return {
      contract,
      principal: principal ?? contract,
      binary: this.graphs[0].binary,
      declared: cs.map((c) => ({
        implementation: c.impl,
        nominal: c.nominal,
        selfPattern: c.selfPattern,
        explicitMethod: c.methods.find((m) => m.member.id === member.id)?.body ?? null,
      })),
      compiled: instances
        .sort((a, b) => a.typeId.localeCompare(b.typeId))
        .map((t) => ({ typeId: t.typeId, name: t.typeName, nominal: t.nominal })),
    };
  }
}
