import type { Backups } from "@sample/backup-api";
import { Label } from "@sample/backup-api";
import { now } from "clock";

export class OsBackups implements Backups {
  write(name: string): string {
    return stamp(name);
  }

  prune(keep: number): void {
    console.log(`keeping ${keep}`);
  }
}

function stamp(name: string): string {
  return `${name}-${now()}`;
}
