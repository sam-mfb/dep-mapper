// Project one supplier's evidence out of a collected build. Stream the
// underlying occurrences so large dependency graphs need no second full index.
import { createReadStream, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';

type Item = { id: string; unit: string; crate: string; name: string };
const [dir, supplier] = process.argv.slice(2);
if (!dir || !supplier) throw new Error('usage: node query.ts REPORT SUPPLIER-PACKAGE-OR-UNIT');
const units = new Set<string>();
const files = readdirSync(join(dir, 'facts')).sort();
async function* facts(file: string) {
  const lines = createInterface({
    input: createReadStream(join(dir, 'facts', file)),
    crlfDelay: Infinity,
  });
  const ordered: string[] = [];
  for await (const line of lines) ordered.push(line);
  for (const line of ordered.sort()) yield JSON.parse(line);
}
for (const fact of JSON.parse(readFileSync(join(dir, 'crates.json'), 'utf8'))) {
  if ([fact.package, fact.owner.crate, fact.owner.unit].includes(supplier)) units.add(fact.owner.unit);
}
if (!units.size) throw new Error('supplier absent from indexed crates: ' + supplier);
const selected = (item?: Item) => !!item && units.has(item.unit);
const emit = (kind: string, evidence: unknown) => console.log(JSON.stringify({ kind, evidence }));
for (const file of files) {
  for await (const fact of facts(file)) {
    if (
      fact.kind === 'impl' &&
      (selected(fact.owner) || selected(fact.provider) || selected(fact.contract))
    )
      emit('implements', fact);
    if (['path', 'use', 'public-use'].includes(fact.kind) && selected(fact.target))
      emit(fact.kind, fact);
    if (fact.kind !== 'body') continue;
    for (const op of fact.ops) {
      if (selected(op.flow?.provider)) emit('supply', { owner: fact.owner, at: op.at, ...op.flow });
      if (
        !selected(fact.owner) &&
        (selected(op.target) || selected(op.selected) || selected(op.nominalSelf))
      )
        emit('call', { owner: fact.owner, ...op });
    }
  }
}
for (const route of JSON.parse(readFileSync(join(dir, 'routes.json'), 'utf8'))) {
  for (const provider of route.providers) {
    const implementation = provider.implementation;
    const evidence = {
      consumer: route.owner,
      invocation: route.at,
      member: route.member,
      provider,
      unresolved: route.stops,
    };
    if (selected(provider.item)) emit('injected-implementation', evidence);
    if (implementation.externalReturnSources?.hits.some((h: { item: Item }) => selected(h.item))) {
      emit('adapter-returns-supplier-result', evidence);
    } else if (implementation.calls?.some((c: { target: Item }) => selected(c.target))) {
      emit('injected-method-calls-supplier', evidence);
    }
  }
}
