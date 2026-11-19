// Finite, context-insensitive graph of stored and forwarded values. A field
// vertex denotes the declaration across all its instances. Every edge has a
// compiler observation. Reachability in this graph does not imply execution.
import { createReadStream, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { liveBlock } from './conditions.ts';
import { balanced, explanation, type Edge } from './balanced.ts';
import { Bounds, type Item } from './bounds.ts';

const dir = process.argv[2];
type Place = {
  local: number;
  fields?: (number | string)[];
  indirect?: boolean;
  opaque?: boolean;
  slot?: string;
};
type Flow = {
  from?: Place;
  environment?: Item;
  coroutine?: boolean;
  parts?: (Place | null)[];
  variant?: string;
  provider?: Item;
  typeParam?: number;
  contract?: Item;
};
type Op = {
  bb: number;
  at: string;
  dst?: Place;
  flow?: Flow;
  fieldDefs?: Item[];
  borrowed?: Place;
  payload?: { out: (number | string)[]; input: (number | string)[]; action: string };
  pointerOp?: string;
  call?: string;
  target?: Item;
  closure?: Item;
  selected?: Item;
  principal?: Item;
  contract?: Item;
  args?: (Place | null)[];
  nominalArgs?: Record<string, Item>;
};
type Body = { owner: Item; argc: number; exposed: number[]; predecessors: number[][]; ops: Op[] };
type Step = { body: string; crate: string; at: string; via: string };
type Arc = Edge<Step>;
type Seed = { provider: Item; contract: Item; step: Step };
const inspected = new Set<string>();
const bounds = new Bounds(),
  bodies = new Map<string, Body>(),
  units = new Set<string>();
const bindings: { owner: Item; emitter: string; nominalArgs: Record<string, Item> }[] = [];
for (const file of readdirSync(join(dir, 'facts')).sort()) {
  for await (const line of createInterface({
    input: createReadStream(join(dir, 'facts', file)),
    crlfDelay: Infinity,
  })) {
    const f = JSON.parse(line);
    bounds.add(f);
    if (f.kind === 'crate') inspected.add(f.owner.unit);
    if (f.kind === 'body') bodies.set(f.owner.id, f);
    if (f.kind === 'executable-graph') for (const u of f.units) units.add(u);
    if (f.kind === 'compiled-bindings') bindings.push(f);
  }
}
bounds.finish(true);
const index = new Map<string, number>(),
  edges: Arc[][] = [],
  seeds = new Map<number, Seed[]>();
function node(key: string) {
  let id = index.get(key);
  if (id === undefined) {
    id = edges.length;
    index.set(key, id);
    edges.push([]);
  }
  return id;
}
// A generic standard-library field such as Result::Ok.0 spans countless
// unrelated types. Use its actual aggregate/projection; never a global slot.
const slots = new Set<string>();
for (const b of bodies.values())
  for (const op of b.ops) {
    for (const field of op.fieldDefs ?? []) if (inspected.has(field.unit)) slots.add(field.id);
    if (op.flow?.environment)
      op.flow.parts?.forEach((_, i) => slots.add(op.flow!.environment!.id + ':capture:' + i));
  }
const wired = new Set<string>();
function edge(dest: number | undefined, src: number | undefined, at: Step) {
  if (dest !== undefined && src !== undefined && dest !== src) edges[dest].push({ to: src, step: at });
}
function transfer(
  dest: number | undefined,
  src: number | undefined,
  remove: (number | string)[],
  add: (number | string)[],
  step: Step,
) {
  if (dest === undefined || src === undefined) return;
  const ops = [
    ...remove.map((t) => ({ pop: String(t) })),
    ...add
      .slice()
      .reverse()
      .map((t) => ({ push: String(t) })),
  ];
  if (!ops.length) {
    edge(dest, src, step);
    return;
  }
  let prev = dest;
  ops.forEach((op, i) => {
    const to = i === ops.length - 1 ? src : edges.length;
    if (i !== ops.length - 1) edges.push([]);
    edges[prev].push({ to, step, ...op });
    prev = to;
  });
}
function storage(b: Item, p: Place | null | undefined, at?: Step, write = false) {
  if (!p || p.opaque) return;
  const base = node(JSON.stringify([b.id, p.local]));
  if (!p.fields?.length) return base;
  const id = node(JSON.stringify([b.id, p.local, p.fields]));
  const key = [id, write, p.slot].join('|');
  if (at && !wired.has(key)) {
    wired.add(key);
    if (write) transfer(base, id, p.fields, [], at);
    else transfer(id, base, [], p.fields, at);
    if (p.slot && slots.has(p.slot)) {
      if (write) edge(node(p.slot), id, at);
      else edge(id, node(p.slot), at);
    }
  }
  return id;
}
const typeArgs = new Map<string, Map<number, Map<string, Item>>>();
for (const b of bindings)
  if (units.has(b.emitter)) {
    if (!typeArgs.has(b.owner.id)) typeArgs.set(b.owner.id, new Map());
    const args = typeArgs.get(b.owner.id)!;
    for (const [idx, v] of Object.entries(b.nominalArgs)) {
      const n = Number(idx);
      if (!args.has(n)) args.set(n, new Map());
      args.get(n)!.set(v.id, v);
    }
  }
function seed(dest: number | undefined, contract: Item, provider: Item, step: Step) {
  if (dest === undefined) return;
  if (!seeds.has(dest)) seeds.set(dest, []);
  seeds.get(dest)!.push({ contract, provider, step });
}
const readers = new Map<string, { node: number; contract: Item }>();
for (const b of bodies.values()) {
  if (!units.has(b.owner.unit)) continue;
  for (const op of b.ops) {
    if (!liveBlock(b, op.bb)) continue;
    const here = (via: string): Step => ({ body: b.owner.name, crate: b.owner.crate, at: op.at, via });
    const dest = storage(b.owner, op.dst, here('write place'), true),
      f = op.flow;
    if (op.borrowed)
      edge(
        dest,
        storage(b.owner, op.borrowed, here('borrowed field')),
        here('mutable borrow; possible value'),
      );
    if (f?.coroutine && f.environment)
      transfer(
        dest,
        storage(f.environment, { local: 0 }),
        ['completion'],
        [],
        here('future completion'),
      );
    if (f?.from) edge(dest, storage(b.owner, f.from, here('read field')), here('move or borrow'));
    if (f?.contract && f.provider) seed(dest, f.contract, f.provider, here('concrete coercion'));
    if (f?.contract && f.typeParam !== undefined) {
      for (const t of typeArgs.get(b.owner.id)?.get(f.typeParam)?.values() ?? [])
        seed(dest, f.contract, t, here('coercion in compiled generic instance'));
    }
    f?.parts?.forEach((p, i) => {
      const field = op.fieldDefs?.[i];
      const cell = field
        ? slots.has(field.id)
          ? node(field.id)
          : undefined
        : f.environment
          ? node(f.environment.id + ':capture:' + i)
          : op.dst
            ? storage(b.owner, {
                ...op.dst,
                fields: [...(op.dst.fields ?? []), ...(f.variant ? [f.variant] : []), i],
              })
            : undefined;
      const source = storage(b.owner, p, here('aggregate operand'));
      transfer(dest, source, [...(f.variant ? [f.variant] : []), i], [], here('aggregate payload ' + i));
      edge(cell, source, here(field ? 'initialize ' + field.name : 'capture or aggregate element'));
    });
    if (!op.call || !op.target) continue;
    if (op.payload)
      transfer(
        dest,
        storage(b.owner, op.args?.[0], here('payload argument')),
        op.payload.out,
        op.payload.input,
        here(op.payload.action),
      );
    if (op.pointerOp)
      edge(
        dest,
        storage(b.owner, op.args?.[0], here('pointer argument')),
        here('standard pointer ' + op.pointerOp),
      );
    const targets = new Map<string, Item>();
    const direct = op.closure ?? op.selected ?? (op.call === 'direct' ? op.target : undefined);
    if (direct) targets.set(direct.id, direct);
    if (op.call === 'dynamic' && op.contract) {
      const receiver = storage(b.owner, op.args?.[0], here('receiver field'));
      if (receiver !== undefined)
        readers.set(b.owner.id + '@' + op.bb, { node: receiver, contract: op.contract });
      for (const c of bounds.candidates(op.contract, op.principal) ?? []) {
        const target = c.methods.find((m) => m.member.id === op.target!.id)?.body ?? op.target;
        targets.set(target.id, target);
      }
    }
    for (const target of targets.values()) {
      const callee = bodies.get(target.id);
      if (!callee) continue;
      const action = op.call === 'dynamic' ? 'candidate virtual call' : 'call';
      edge(dest, storage(target, { local: 0 }), here(action + ' return'));
      for (let n = 1; n <= callee.argc; n++) {
        const arg = op.args?.[op.closure && n >= 2 ? 1 : n - 1];
        const actual =
          op.closure && n >= 2 && arg
            ? { ...arg, fields: [...(arg.fields ?? []), n - 2], slot: undefined }
            : arg;
        edge(
          storage(target, { local: n }),
          storage(b.owner, actual, here('argument projection')),
          here(action + ' argument ' + n),
        );
      }
    }
  }
}
// Search through cycles to exhaustion. Retain one explanation per cast
// and concrete type; joins through different instances of a field stay visible.
const routes = JSON.parse(readFileSync(join(dir, 'routes.json'), 'utf8'));
const output = [];
const reverse: number[][] = edges.map(() => []);
edges.forEach((arcs, n) => arcs.forEach((arc) => reverse[arc.to].push(n)));
const relevant = new Map<string, Set<number>>();
function slice(contract: Item) {
  let cached = relevant.get(contract.id);
  if (cached) return cached;
  const todo: number[] = [],
    seen = new Set<number>();
  for (const [n, values] of seeds)
    if (values.some((s) => bounds.satisfies(s.contract, contract))) {
      todo.push(n);
      seen.add(n);
    }
  for (let i = 0; i < todo.length; i++)
    for (const src of reverse[todo[i]])
      if (!seen.has(src)) {
        seen.add(src);
        todo.push(src);
      }
  relevant.set(contract.id, seen);
  return seen;
}
for (const r of routes) {
  if (!r.bound) continue;
  const reader = readers.get(r.owner.id + '@' + r.block);
  const paths = new Map<string, { provider: Item; steps: Step[] }>();
  if (reader) {
    const allowed = slice(reader.contract);
    if (allowed.has(reader.node))
      for (const [v, proof] of balanced(edges, reader.node, allowed)) {
        for (const s of seeds.get(v) ?? []) {
          if (!bounds.satisfies(s.contract, reader.contract)) continue;
          const key = JSON.stringify([s.provider.id, s.step.crate, s.step.body, s.step.at]);
          if (paths.has(key)) continue;
          const steps = [s.step, ...explanation(proof).reverse()].filter(
            (step, i, arr) => !i || step !== arr[i - 1],
          );
          paths.set(key, { provider: s.provider, steps });
        }
      }
  }
  output.push({
    owner: r.owner,
    block: r.block,
    at: r.at,
    member: r.member,
    paths: [...paths.values()],
  });
}
writeFileSync(join(dir, 'structural.json'), JSON.stringify(output));
console.log(
  'Finite storage graph:',
  edges.length,
  'vertices,',
  edges.reduce((n, e) => n + e.length, 0),
  'transfers,',
  output.filter((r) => r.paths.length).length,
  'of',
  output.length,
  'calls with a path',
);
