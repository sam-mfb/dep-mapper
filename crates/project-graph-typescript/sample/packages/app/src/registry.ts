import type { Backups } from "@sample/backup-api";
import { OsBackups } from "@sample/backup-os";

// An implementation written as a typed object literal, not a class.
export const quietBackups: Backups = {
  write: (label) => label,
  prune: () => {},
};

const ALL: Backups[] = [quietBackups, new OsBackups()];

// The registry is reached through `||`, an array method's callback,
// a `for...of` loop and `find`.
export function writeAll(choice?: Backups[]): void {
  const list = choice || ALL;
  list.map((each) => each.write("all"));
  for (const item of list) {
    item.prune(1);
  }
  const first = list.find((candidate) => candidate.write("probe") !== "");
  first?.write("first");
}

// The fallback is used only when no backups are given.
export function writeOne(given?: Backups): string {
  const chosen = given || quietBackups;
  return chosen.write("one");
}

class Tally {
  count(): void {}
}

// reduce passes its accumulator first and an element second.
export function tallyAll(): Tally {
  return ALL.reduce((total: Tally, next) => {
    total.count();
    next.prune(0);
    return total;
  }, new Tally());
}

// An array pattern over a value that is not an array literal.
export function writePrimary(): string {
  const [primary, second = quietBackups] = ALL;
  second.prune(2);
  return primary.write("primary");
}

class FallbackBackups implements Backups {
  write(label: string): string {
    return label;
  }

  prune(_keep: number): void {}
}

interface Holder {
  backups: Backups;
  spare?: Backups;
}

// Destructuring in assignments, not declarations.
export function reassign(): string {
  const holder: Holder = { backups: new OsBackups() };
  let picked: Backups;
  let spare: Backups;
  ({ backups: picked, spare = quietBackups } = holder);
  let first: Backups;
  let second: Backups;
  [first, second = new FallbackBackups()] = ALL;
  second.prune(0);
  return picked.write("reassigned") + spare.write("held") + first.write("in order");
}

// A name bound by destructuring, reassigned through a shorthand pattern.
export function rebind(): string {
  const holder: Holder = { backups: new OsBackups() };
  let { backups } = holder;
  ({ backups } = holder);
  return backups.write("rebound");
}

// An assignment written inside a shorthand default is a real assignment.
export function lateFallback(): Backups {
  const holder: Holder = { backups: new OsBackups() };
  let late: Backups = new FallbackBackups();
  let spare: Backups;
  ({ spare = (late = quietBackups) } = holder);
  spare.prune(3);
  return late;
}
