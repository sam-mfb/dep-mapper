import type { Backups } from "@sample/backup-api";

export type { Backups } from "@sample/backup-api";

export class Installer {
  constructor(private readonly backups: Backups) {}

  install(name: string): string {
    return this.backups.write(name);
  }
}

// No code in the project calls this, so its receiver's origin is unknown.
export function installWith(backups: Backups, name: string): string {
  return backups.write(name);
}
