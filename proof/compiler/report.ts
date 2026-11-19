// Read compiler records for one build. Walk backward from each dynamic call's
// receiver. Retain a source location for each handoff and each place we stop.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { alternatives, liveBlock, possibleEdge, constrain, type Condition } from './conditions.ts';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { createReadStream } from 'node:fs';

type Item = { id: string; crate: string; unit: string; name: string };
type Place = { local: number; fields?: (number | string)[]; opaque?: boolean; indirect?: boolean };
type Flow = {
  from?: Place;
  parts?: (Place | null)[];
  typeParam?: number;
  variant?: string;
  environment?: Item;
  negated?: Place;
  provider?: Item;
  contract?: Item;
  unknown?: string;
};
type Op = {
  bb: number;
  at: string;
  dst?: Place;
  nominalArgs?: Record<string, Item>;
  pointerOp?: string;
  borrowed?: Place;
  flow?: Flow;
  call?: string;
  target?: Item;
  contract?: Item;
  closure?: Item;
  selected?: Item;
  args?: (Place | null)[];
};
type Body = {
  kind: 'body';
  owner: Item;
  argc: number;
  exposed: number[];
  predecessors: number[][];
  returns: number[];
  ops: Op[];
};
type Impl = {
  kind: 'impl';
  simpleImpl: boolean;
  owner: Item;
  at: string;
  typeParam?: number;
  variant?: string;
  environment?: Item;
  negated?: Place;
  provider?: Item;
  contract: Item;
  methods: { body: Item; member: Item }[];
};
type Site = { body: Body; op: Op; index: number };
type Step = { owner: Item; at: string; block?: number };
type Unknown = Step & { reason: string };
type Hit = Step & { item: Item; steps: Step[]; conditions?: Condition[] };
type Result = { hits: Hit[]; stops: Unknown[]; excluded: Unknown[] };
type Point = {
  body: Body;
  block: number;
  before: number;
  place: Place;
  stack: Site[];
  steps: Step[];
  conditions?: Condition[];
  epoch?: number;
};
const dir = process.argv[2];
const omit = new Set(process.argv.slice(3));
const captures = new Map<string, Site[]>();
const bodies = new Map<string, Body>();
const expectedBodies = new Map<string, Item>();
const callers = new Map<string, Site[]>();
const impls: Impl[] = [];
const counts: Record<string, number> = {};
const callCounts: Record<string, number> = {};
const increment = (map: Record<string, number>, key: string) => (map[key] = (map[key] ?? 0) + 1);
const calls: Site[] = [];
const crates: { package: string; owner: Item }[] = [];
const byBlock = new Map<Body, Map<number, number[]>>();
for (const file of readdirSync(join(dir, 'facts')).sort()) {
  const lines = createInterface({
    input: createReadStream(join(dir, 'facts', file)),
    crlfDelay: Infinity,
  });
  for await (const line of lines) {
    const fact = JSON.parse(line);
    increment(counts, fact.kind);
    if (fact.kind === 'mir-keys') {
      for (const member of fact.items) expectedBodies.set(member.id, member);
    }
    if (fact.kind === 'crate') crates.push(fact);
    if (fact.kind === 'impl') impls.push(fact);
    if (fact.kind !== 'body') continue;
    const body: Body = fact;
    if (bodies.has(body.owner.id)) throw new Error('duplicate compiled body: ' + body.owner.name);
    bodies.set(body.owner.id, body);
    const blocks = new Map<number, number[]>();
    byBlock.set(body, blocks);
    body.ops.forEach((op, index) => {
      if (!blocks.has(op.bb)) blocks.set(op.bb, []);
      blocks.get(op.bb)!.push(index);
      if (op.flow?.environment && !omit.has('--without-captures')) {
        const id = op.flow.environment.id;
        if (!captures.has(id)) captures.set(id, []);
        captures.get(id)!.push({ body, op, index });
      }
      if (!op.call) return;
      increment(callCounts, op.call);
      const site = { body, op, index };
      calls.push(site);
      const callee = op.closure ?? op.selected ?? (op.call === 'direct' ? op.target : undefined);
      if (!callee) return;
      if (!callers.has(callee.id)) callers.set(callee.id, []);
      callers.get(callee.id)!.push(site);
    });
  }
}
if (!crates.length || counts['mir-keys'] !== crates.length) {
  throw new Error('incomplete compiler inventory: each indexed crate must report its MIR keys');
}
for (const sites of callers.values()) {
  sites.sort((a, b) => a.body.owner.id.localeCompare(b.body.owner.id) || a.index - b.index);
}
const step = (b: Body, op: Op): Step => ({ owner: b.owner, at: op.at, block: op.bb });
const last = Number.MAX_SAFE_INTEGER;
const matches = (prefix: (number | string)[], fields: (number | string)[]) =>
  prefix.every((v, i) => fields[i] === v);

// A direct call frame pairs callee returns with actuals at that particular call.
// Without a frame, enumerate in-scope callers. Each result is a structural witness.
function origins(start: Point[], contract?: Item): Result {
  const hits = new Map<string, Hit>();
  const stops = new Map<string, Unknown>();
  const excluded = new Map<string, Unknown>();
  function ruleOut(p: Point, reason: string, op?: Op) {
    const site = op ? step(p.body, op) : (p.steps.at(-1) ?? { owner: p.body.owner, at: 'entry' });
    excluded.set(JSON.stringify([site.owner.id, site.at, reason]), { ...site, reason });
  }
  const visited = new Set<string>();
  const queue = [...start];
  function stop(p: Point, reason: string, op?: Op) {
    const here = op
      ? step(p.body, op)
      : (p.steps.at(-1) ?? { owner: p.body.owner, at: 'function entry' });
    stops.set(JSON.stringify([here.owner.id, here.at, reason]), { ...here, reason });
  }
  for (let cursor = 0; cursor < queue.length; cursor++) {
    let p = queue[cursor];
    const { body, block, before, place, stack, steps } = p;
    if (cursor >= 6000) {
      stop(p, 'search budget');
      break;
    }
    if (stack.length > 12 || steps.length > 80) {
      stop(p, 'path depth budget');
      continue;
    }
    if (body.exposed.includes(place.local)) {
      stop(p, 'storage exposed by raw address');
      continue;
    }
    if (place.opaque || !place.fields) {
      stop(p, 'unsupported place projection');
      continue;
    }
    const guardAt = (bb: number) =>
      contract && !omit.has('--without-guards')
        ? constrain(p.conditions ?? [], body, bb, stack, p.epoch ?? 0)
        : [];
    if (contract && !omit.has('--without-guards') && !liveBlock(body, block)) {
      ruleOut(p, 'block excluded by build-time Boolean', body.ops[before]);
      continue;
    }
    if (before !== last) {
      const conditions = guardAt(block);
      if (!conditions) {
        ruleOut(p, 'opposite requirements on the same Boolean', body.ops[before]);
        continue;
      }
      p = { ...p, conditions };
    }
    const key = JSON.stringify([
      body.owner.id,
      block,
      before,
      place.local,
      place.fields,
      p.conditions?.map((c) => [c.activation, c.local, c.value]).sort(),
      stack.map((s) => [s.body.owner.id, s.index]),
    ]);
    if (visited.has(key)) continue;
    visited.add(key);
    const fields = place.fields;
    const indices = byBlock.get(body)!.get(block) ?? [];
    let found = false;
    for (let k = indices.length - 1; k >= 0; k--) {
      const index = indices[k],
        op = body.ops[index],
        dst = op.dst;
      if (index >= before) continue;
      const borrow = op.borrowed;
      if (
        borrow?.local === place.local &&
        (!borrow.fields || matches(borrow.fields, fields) || matches(fields, borrow.fields))
      ) {
        found = true;
        stop(p, 'mutable borrow on incoming path', op);
        break;
      }
      if (!dst || dst.local !== place.local) continue;
      if (dst.fields && !matches(dst.fields, fields) && !matches(fields, dst.fields)) continue;
      found = true;
      if (!dst.fields || dst.indirect || dst.fields.length > fields.length) {
        stop(p, 'write through address or subfield', op);
        break;
      }
      const conditions = guardAt(block);
      if (!conditions) {
        ruleOut(p, 'opposite requirements on the same Boolean', op);
        break;
      }
      p = { ...p, conditions };
      const rest = fields.slice(dst.fields.length);
      const flow = op.flow;
      function follow(source: Place | null | undefined, suffix: (number | string)[], record = false) {
        if (!source) {
          stop(p, 'operand without tracked storage', op);
          return;
        }
        queue.push({
          ...p,
          before: index,
          place: { ...source, fields: [...(source.fields ?? []), ...suffix] },
          steps: record ? [...steps, step(body, op)] : steps,
        });
      }
      if (flow?.provider && flow.contract?.id === contract?.id && rest.length === 0) {
        hits.set(JSON.stringify([body.owner.id, index, conditions]), {
          ...step(body, op),
          item: flow.provider,
          steps,
          conditions,
        });
      } else if (flow?.typeParam !== undefined && contract?.id === flow.contract?.id && !rest.length) {
        const sites = stack.length ? stack.slice(-1) : (callers.get(body.owner.id) ?? []);
        if (!sites.length || omit.has('--without-generics')) stop(p, 'unbound type parameter', op);
        else
          for (const site of sites) {
            const concrete = site.op.nominalArgs?.[flow.typeParam];
            if (site.op.closure || !concrete) {
              stop({ ...p, body: site.body }, 'caller has no concrete type argument', site.op);
              continue;
            }
            const active = !omit.has('--without-guards');
            if (active && !liveBlock(site.body, site.op.bb)) continue;
            const guards = active
              ? constrain(
                  conditions,
                  site.body,
                  site.op.bb,
                  stack.slice(0, -1),
                  (p.epoch ?? 0) + (stack.length ? 0 : 1),
                )
              : conditions;
            if (!guards) continue;
            hits.set(
              JSON.stringify([
                body.owner.id,
                index,
                concrete.id,
                site.body.owner.id,
                site.index,
                guards,
              ]),
              {
                ...step(body, op),
                item: concrete,
                steps: [...steps, step(site.body, site.op)],
                conditions: guards,
              },
            );
          }
      } else if (contract && op.pointerOp && !omit.has('--without-pointers')) {
        follow(op.args?.[0], rest, true);
      } else if (flow?.from) {
        follow(flow.from, rest);
      } else if (flow?.variant && flow.parts) {
        if (omit.has('--without-variants') || typeof rest[0] !== 'string')
          stop(p, 'enum payload not selected', op);
        else if (flow.variant === rest[0] && typeof rest[1] === 'number')
          follow(flow.parts[rest[1]], rest.slice(2), true);
      } else if (flow?.parts && typeof rest[0] === 'number') {
        follow(flow.parts[rest[0]], rest.slice(1), true);
      } else if ((op.call === 'direct' || op.closure || op.selected) && op.target) {
        const target = op.closure ?? op.selected ?? op.target;
        if (!contract && target.unit !== start[0].body.owner.unit) {
          hits.set(body.owner.id + '@' + index, { ...step(body, op), item: target, steps });
        } else {
          const callee = bodies.get(target.id);
          if (!callee || !callee.returns.length) stop(p, 'no indexed return', op);
          for (const exit of callee?.returns ?? [])
            queue.push({
              ...p,
              body: callee!,
              block: exit,
              before: last,
              place: { local: 0, fields: rest },
              stack: [...stack, { body, op, index }],
              steps: [...steps, step(body, op)],
            });
        }
      } else
        stop(p, flow?.unknown ?? (op.call ? op.call + ' call result' : 'unmodelled definition'), op);
      break;
    }
    if (found) continue;
    for (const pred of body.predecessors[block]) {
      if (contract && !omit.has('--without-guards') && !possibleEdge(body, pred, block)) continue;
      queue.push({ ...p, block: pred, before: last });
    }
    if (block !== 0) continue;
    if (place.local === 0 || place.local > body.argc) {
      stop(p, 'no incoming definition');
      continue;
    }
    if (!contract && !stack.length) {
      stop(p, 'method returns an incoming argument');
      continue;
    }
    const sites = stack.length ? stack.slice(-1) : (callers.get(body.owner.id) ?? []);
    if (
      !sites.length &&
      contract &&
      place.local === 1 &&
      typeof fields[0] === 'number' &&
      captures.has(body.owner.id)
    ) {
      for (const creator of captures.get(body.owner.id)!) {
        const value = creator.op.flow!.parts![fields[0]];
        if (!value) {
          stop({ ...p, body: creator.body }, 'capture without tracked storage', creator.op);
          continue;
        }
        queue.push({
          ...p,
          epoch: (p.epoch ?? 0) + 1,
          body: creator.body,
          block: creator.op.bb,
          before: creator.index,
          place: { ...value, fields: [...(value.fields ?? []), ...fields.slice(1)] },
          steps: [...steps, step(creator.body, creator.op)],
        });
      }
      continue;
    }
    if (!sites.length) stop(p, 'no indexed direct caller');
    for (const site of sites) {
      const tupleField = site.op.closure && place.local >= 2 ? [place.local - 2] : [];
      const actual = site.op.args?.[tupleField.length ? 1 : place.local - 1];
      if (!actual) {
        stop({ ...p, body: site.body }, 'caller operand without tracked storage', site.op);
        continue;
      }
      queue.push({
        ...p,
        epoch: (p.epoch ?? 0) + (stack.length ? 0 : 1),
        body: site.body,
        block: site.op.bb,
        before: site.index,
        place: { ...actual, fields: [...(actual.fields ?? []), ...tupleField, ...fields] },
        stack: stack.slice(0, -1),
        steps: [...steps, step(site.body, site.op)],
      });
    }
  }
  return { hits: [...hits.values()], stops: [...stops.values()], excluded: [...excluded.values()] };
}

const returns = new Map<string, Result>();
function implementation(hit: Hit, member: Item) {
  const candidates = impls
    .filter((i) => i.simpleImpl && i.provider?.id === hit.item.id)
    .flatMap((i) => i.methods.filter((m) => m.member.id === member.id));
  if (candidates.length !== 1) return { unknown: 'no unique explicit implementation body' };
  const method = candidates[0].body;
  const body = bodies.get(method.id);
  if (!body) return { method, unknown: 'implementation body outside inventory' };
  if (!returns.has(method.id)) {
    returns.set(
      method.id,
      origins(
        body.returns.map((block) => ({
          body,
          block,
          before: last,
          place: { local: 0, fields: [] },
          stack: [],
          steps: [],
        })),
      ),
    );
  }
  const calls = body.ops
    .filter((op) => op.target && op.target.unit !== method.unit)
    .map((op) => ({ kind: op.call, at: op.at, target: op.target }));
  return { method, calls, externalReturnSources: returns.get(method.id) };
}

const routes = calls
  .filter((s) => s.op.call === 'dynamic')
  .map(({ body, op, index }) => {
    const receiver = op.args?.[0];
    const result = receiver
      ? origins(
          [{ body, block: op.bb, before: index, place: receiver, stack: [], steps: [step(body, op)] }],
          op.contract!,
        )
      : {
          hits: [],
          excluded: [],
          stops: [{ ...step(body, op), reason: 'receiver without tracked storage' }],
        };
    return {
      ...step(body, op),
      block: op.bb,
      member: op.target,
      providers: result.hits.map((hit) => ({
        ...hit,
        conditions: hit.conditions?.map(({ activation, ...condition }) => condition),
        enumChoices: omit.has('--without-guards')
          ? []
          : [
              ...new Map(
                [hit, ...hit.steps]
                  .flatMap((s) =>
                    s.block === undefined ? [] : alternatives(bodies.get(s.owner.id)!, s.block),
                  )
                  .map((g) => [JSON.stringify(g), g]),
              ).values(),
            ],
        implementation: implementation(hit, op.target!),
      })),
      stops: result.stops,
      excluded: result.excluded,
    };
  });
routes.sort((a, b) => a.owner.id.localeCompare(b.owner.id) || a.block - b.block);
const graph = JSON.parse(readFileSync(join(dir, 'units.json'), 'utf8'));
if (graph.version !== 1) throw new Error('unsupported Cargo unit graph');
const indexedUnits = new Set(crates.map((c) => c.owner.unit));
const crossCrate = routes.filter(
  (r) => r.member && indexedUnits.has(r.member.unit) && r.member.unit !== r.owner.unit,
);
const stopsBySite: Record<string, number> = {};
for (const r of routes) {
  if (r.providers.length) continue;
  if (!r.stops.length && !r.excluded.length)
    r.stops.push({ owner: r.owner, at: r.at, reason: 'no origin reached in explored graph' });
  for (const reason of new Set(r.stops.map((s) => s.reason))) increment(stopsBySite, reason);
}
const coverage = {
  listedByRustc: expectedBodies.size,
  collected: bodies.size,
  listedWithoutCollectedBody: [...expectedBodies.values()].filter((i) => !bodies.has(i.id)),
  collectedOutsideListing: [...bodies.values()]
    .filter((b) => !expectedBodies.has(b.owner.id))
    .map((b) => b.owner),
};
writeFileSync(join(dir, 'body-coverage.json'), JSON.stringify(coverage, null, 2));
const summary = {
  units: graph.units.length,
  unitEdges: graph.units.reduce(
    (n: number, u: { dependencies: unknown[] }) => n + u.dependencies.length,
    0,
  ),
  listedByRustc: coverage.listedByRustc,
  missingListedBodies: coverage.listedWithoutCollectedBody.length,
  facts: counts,
  calls: callCounts,
  stopsByUnwitnessedSite: stopsBySite,
  crossIndexedCrateDynamicSites: crossCrate.length,
  crossIndexedCrateWithWitness: crossCrate.filter((r) => r.providers.length).length,
  dynamicSites: routes.length,
  withWitness: routes.filter((r) => r.providers.length).length,
  withMultipleProviders: routes.filter((r) => new Set(r.providers.map((p) => p.item.id)).size > 1)
    .length,
  withoutWitness: routes.filter((r) => !r.providers.length).length,
};
writeFileSync(
  join(dir, 'crates.json'),
  JSON.stringify(
    crates.sort((a, b) => a.owner.unit.localeCompare(b.owner.unit)),
    null,
    2,
  ),
);
writeFileSync(join(dir, 'routes.json'), JSON.stringify(routes, null, 2));
writeFileSync(join(dir, 'summary.json'), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
