// A Boolean is correlated only when it is an immutable formal parameter or a
// chain of whole-local copies/negations of one. Other switches remain structural.
type Place = { local: number; fields?: (number | string)[]; opaque?: boolean; indirect?: boolean };
type Instruction = {
  bb: number;
  dst?: Place;
  borrowed?: Place;
  flow?: { from?: Place; negated?: Place; boolean?: boolean };
  args?: (Place | null)[];
  closure?: unknown;
};
type Switch = { bb: number; local: number; yes: number; no: number; at: string };
type Body = {
  owner: { id: string; name: string; crate: string };
  ops: Instruction[];
  argc: number;
  switches?: Switch[];
  enumSwitches?: { bb: number; at: string; enumName: string; cases: { name: string; to: number }[] }[];
  labels?: Record<string, string>;
  exposed: number[];
  predecessors: number[][];
};
type Site = { body: Body; op: Instruction; index: number };
type Formal = { local: number; inverted: boolean };
export type Condition = {
  activation: string;
  owner: string;
  crate: string;
  local: number;
  name: string;
  value: boolean;
  at: string;
};
type Alternative = { at: string; enumName: string; variants: string[] };
type Info = {
  formal: (local: number) => Formal | undefined;
  blocks: Map<number, Guard[]>;
  deadEdges: Map<number, number>;
  live?: Set<number>;
  alternatives: Map<number, Alternative[]>;
};
type Guard = { formal: Formal; at: string; value: boolean };
const cache = new WeakMap<Body, Info>();
function plain(p?: Place | null) {
  return p && !p.opaque && !p.indirect && p.fields?.length === 0;
}

function analyze(body: Body) {
  const known = cache.get(body);
  if (known) return known;
  const defs = new Map<number, Instruction[]>();
  const unstable = new Set(body.exposed);
  for (const op of body.ops) {
    if (op.borrowed) unstable.add(op.borrowed.local);
    if (!op.dst) continue;
    if (!plain(op.dst)) unstable.add(op.dst.local);
    const list = defs.get(op.dst.local) ?? [];
    list.push(op);
    defs.set(op.dst.local, list);
  }
  function formal(local: number): Formal | undefined {
    const seen = new Set<number>();
    let inverted = false;
    while (!seen.has(local) && !unstable.has(local)) {
      seen.add(local);
      const writes = defs.get(local) ?? [];
      if (!writes.length && local > 0 && local <= body.argc) return { local, inverted };
      if (writes.length !== 1) return;
      const flow = writes[0].flow,
        input = flow?.from ?? flow?.negated;
      if (!plain(input)) return;
      if (flow?.negated) inverted = !inverted;
      local = input!.local;
    }
  }
  function constant(local: number): boolean | undefined {
    const seen = new Set<number>();
    let invert = false;
    while (!seen.has(local) && !unstable.has(local)) {
      seen.add(local);
      if (local <= body.argc) return;
      const writes = defs.get(local) ?? [];
      if (writes.length !== 1) return;
      const f = writes[0].flow;
      if (typeof f?.boolean === 'boolean') return f.boolean !== invert;
      const source = f?.from ?? f?.negated;
      if (!plain(source)) return;
      if (f?.negated) invert = !invert;
      local = source!.local;
    }
  }
  const blocks = new Map<number, Guard[]>();
  const deadEdges = new Map<number, number>();
  const alternatives = new Map<number, Alternative[]>();
  let live: Set<number> | undefined;
  for (const sw of body.switches ?? []) {
    const value = constant(sw.local);
    if (value !== undefined) deadEdges.set(sw.bb, value ? sw.no : sw.yes);
  }
  if (body.switches?.length || body.enumSwitches?.length) {
    const successors: number[][] = body.predecessors.map(() => []);
    body.predecessors.forEach((preds, block) => preds.forEach((pred) => successors[pred].push(block)));
    function reachable(skipFrom = -1, skipTo = -1) {
      const seen = new Set<number>([0]),
        todo = [0];
      while (todo.length) {
        const from = todo.pop()!;
        for (const to of successors[from]) {
          if (deadEdges.get(from) === to || (from === skipFrom && to === skipTo) || seen.has(to))
            continue;
          seen.add(to);
          todo.push(to);
        }
      }
      return seen;
    }
    const all = reachable();
    if (deadEdges.size) live = all;
    for (const sw of body.enumSwitches ?? []) {
      const groups = new Map<number, string[]>();
      for (const c of sw.cases) {
        const g = groups.get(c.to) ?? [];
        g.push(c.name);
        groups.set(c.to, g);
      }
      if (groups.size < 2) continue;
      for (const [to, variants] of groups) {
        const remainder = reachable(sw.bb, to);
        for (const bb of all)
          if (!remainder.has(bb)) {
            const list = alternatives.get(bb) ?? [];
            list.push({ at: sw.at, enumName: sw.enumName, variants });
            alternatives.set(bb, list);
          }
      }
    }
    for (const sw of body.switches ?? []) {
      const f = formal(sw.local);
      if (!f) continue;
      for (const value of [true, false]) {
        const remainder = reachable(sw.bb, value ? sw.yes : sw.no);
        for (const bb of all) {
          if (remainder.has(bb)) continue;
          const guards = blocks.get(bb) ?? [];
          guards.push({ formal: f, value, at: sw.at });
          blocks.set(bb, guards);
        }
      }
    }
  }
  const info = { formal, blocks, live, deadEdges, alternatives };
  cache.set(body, info);
  return info;
}

export function constrain(
  existing: Condition[],
  body: Body,
  block: number,
  stack: Site[],
  epoch: number,
): Condition[] | undefined {
  const guards = analyze(body).blocks.get(block);
  if (!guards) return existing;
  const result = existing.slice();
  for (const g of guards) {
    let here = body,
      depth = stack.length,
      local = g.formal.local,
      value = g.value !== g.formal.inverted;
    while (depth) {
      const site = stack[depth - 1];
      if (site.op.closure) break; // Rust-call ABI packs arguments in a tuple.
      const actual = site.op.args?.[local - 1];
      if (!plain(actual)) break;
      const outer = analyze(site.body).formal(actual!.local);
      if (!outer) break;
      local = outer.local;
      value = value !== outer.inverted;
      here = site.body;
      depth--;
    }
    const activation = JSON.stringify([
      epoch,
      here.owner.id,
      stack.slice(0, depth).map((s) => [s.body.owner.id, s.index]),
    ]);
    const previous = result.find((c) => c.activation === activation && c.local === local);
    if (previous && previous.value !== value) return;
    if (!previous)
      result.push({
        activation,
        owner: here.owner.name,
        crate: here.owner.crate,
        local,
        name: here.labels?.[local] ?? 'argument ' + local,
        value,
        at: g.at,
      });
  }
  return result;
}

export function liveBlock(body: Body, bb: number) {
  return analyze(body).live?.has(bb) ?? true;
}
export function possibleEdge(body: Body, from: number, to: number) {
  return analyze(body).deadEdges.get(from) !== to;
}
export function alternatives(body: Body, bb: number) {
  return analyze(body).alternatives.get(bb) ?? [];
}
