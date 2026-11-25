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
