import type { Backups } from "@sample/backup-api";
import { OsBackups } from "@sample/backup-os";

class SpareBackups implements Backups {
  write(label: string): string {
    return label;
  }

  prune(_keep: number): void {}
}

function echo(backups: Backups): Backups {
  return backups;
}

// Each result goes straight into another call before it is used.
export function nested(): void {
  const first = echo(echo(new OsBackups()));
  const second = echo(echo(new SpareBackups()));
  first.write("first");
  second.write("second");
}

// Each call's result is the receiver of the next call.
export function temporary(): void {
  echo(new OsBackups()).write("first");
  echo(new SpareBackups()).write("second");
}
