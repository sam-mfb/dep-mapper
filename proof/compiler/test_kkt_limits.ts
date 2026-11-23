// These assertions are an independent oracle for the pinned Linux build.
// Names and source locations here are absent from the analysis.
import assert from 'node:assert/strict';
import { createReadStream, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { liveBlock } from './conditions.ts';

const [report, swapped] = process.argv.slice(2);
const read = (dir: string, name: string) => JSON.parse(readFileSync(join(dir, name), 'utf8'));
const ledger = read(report, 'limits.json');
assert.equal(ledger.build.revision, '41fbc289dbfe68be90840d107e217762229b9745');
assert.deepEqual(ledger.build.selection, ['-p', 'kashkeshet', '--bin', 'kkt', '--lib']);
assert.equal(ledger.sites, 668);
assert.deepEqual(ledger.paths, {
  contextual: 391,
  'structural candidate only': 263,
  'no recovered path': 14,
});
assert.equal(ledger.declarations['one nominal type crate'], 643);
type Row = {
  singleTypeCrate: boolean;
  declarationsWithoutMethod: number;
  contractCrate: string;
  compiledTypes: string[];
  declaredTypeCrates: string[];
  contextualTypes: string[];
  structuralTypes: string[];
  stops: string[];
  evidence: string;
  crate: string;
  member: string;
  source: string;
};
assert.equal((ledger.ledger as Row[]).filter((r) => r.declarationsWithoutMethod > 0).length, 8);
const split = (rows: Row[], contract: string) =>
  rows.filter((r) => !r.singleTypeCrate && r.contractCrate === contract);
const service = split(ledger.ledger, 'kkt_bare_service'),
  login = split(ledger.ledger, 'kkt_containerized_service');
assert.equal(service.length, 23);
assert.equal(login.length, 2);
for (const r of service) assert.deepEqual(r.compiledTypes, ['kkt_bare_service_systemd::Systemd']);
const terminal = 'terminal::browser_login::TerminalLink';
for (const r of login) {
  assert.deepEqual(r.contextualTypes, [terminal]);
  assert.deepEqual(r.stops, []);
  assert.deepEqual(r.structuralTypes, ['NoLogin', terminal]);
}
const missing: Record<string, number> = {};
for (const r of ledger.ledger as Row[])
  if (r.evidence === 'no recovered path') {
    const name = r.member.split('::').at(-2)!;
    missing[name] = (missing[name] ?? 0) + 1;
  }
assert.deepEqual(missing, { Forms: 4, Notices: 1, LaneServer: 3, CredentialSlot: 3, Place: 3 });
const casts: { trait: string; body: string; receiver: string; live: boolean }[] = [];
const calls: Record<string, string[]> = {};
for (const name of readdirSync(join(report, 'facts'))) {
  for await (const line of createInterface({
    input: createReadStream(join(report, 'facts', name)),
    crlfDelay: Infinity,
  })) {
    const b = JSON.parse(line);
    if (b.kind !== 'body') continue;
    for (const op of b.ops) {
      const f = op.flow,
        name = f?.contract?.name.split('::').at(-1);
      if (
        (name === 'Service' && f.contract.crate === 'kkt_bare_service') ||
        (name === 'LoginLink' && f.contract.crate === 'kkt_containerized_service')
      ) {
        casts.push({
          trait: name,
          body: b.owner.name,
          receiver: f.provider?.name ?? 'other',
          live: liveBlock(b, op.bb),
        });
      }
      const target = op.target?.name;
      if (
        [
          'setup::log_in',
          'setup::provision',
          'kkt_containerized_service::ServiceContainer::provision',
        ].includes(target)
      ) {
        (calls[target] ??= []).push(b.owner.name);
      }
    }
  }
}
const system = casts.filter((c) => c.trait === 'Service');
assert.equal(system.length, 3);
assert(system.every((c) => c.body === 'machine::service::for_this_os'));
assert.deepEqual(
  system.filter((c) => c.live).map((c) => c.receiver),
  ['kkt_bare_service_systemd::Systemd'],
);
const origins = casts.filter((c) => c.trait === 'LoginLink');
assert.deepEqual(
  origins.map((c) => [c.body, c.receiver]).sort(),
  [
    ["machine::native::NativeParts::<'a, D>::install_parts", terminal],
    ['<Containerized<P> as container::ServiceContainer>::add_admin_key::{closure#0}', 'NoLogin'],
    ['<Containerized<P> as container::ServiceContainer>::upgrade::{closure#0}', 'NoLogin'],
  ].sort(),
);
assert.deepEqual(calls['setup::log_in'], ['setup::provision::{closure#0}']);
assert.deepEqual(calls['setup::provision'], [
  '<Containerized<P> as container::ServiceContainer>::provision::{closure#0}',
]);
assert.equal(calls['kkt_containerized_service::ServiceContainer::provision'].length, 1);
assert(
  calls['kkt_containerized_service::ServiceContainer::provision'][0].includes('::install_docker::'),
);
if (swapped) {
  const rewired = split(read(swapped, 'limits.json').ledger, 'kkt_containerized_service');
  assert.equal(rewired.length, 2);
  for (const r of rewired) {
    const old = login.find((x) => x.source === r.source)!;
    assert.deepEqual(r.declaredTypeCrates, old.declaredTypeCrates);
    assert.deepEqual([...r.compiledTypes].sort(), [...old.compiledTypes].sort());
    assert.deepEqual(r.contextualTypes, ['NoLogin']);
    assert.deepEqual(r.stops, []);
  }
  console.log(
    'PASS replacing the provision argument changes both receiver paths while both compiled types stay present',
  );
}
console.log(
  'PASS 668 site classifications, Linux factory selection, six coercions, and the sole provisioning chain',
);

const audited = ledger.ledger.map((r: Row) => {
  if (r.singleTypeCrate)
    return { ...r, receiverCrate: r.declaredTypeCrates[0], basis: 'compiled impl declarations' };
  if (r.contractCrate === 'kkt_bare_service')
    return {
      ...r,
      receiverCrate: 'kkt_bare_service_systemd',
      basis: 'audited factory and Linux build selection',
    };
  assert.equal(r.contractCrate, 'kkt_containerized_service');
  return {
    ...r,
    receiverCrate: 'kashkeshet',
    basis: 'audited provisioning chain and call-specific trace',
  };
});
writeFileSync(
  join(report, 'audited-limits.json'),
  JSON.stringify({ ...ledger, ledger: audited }, null, 2) + '\n',
);
