import type { Backups } from "@sample/backup-api";
import { OsBackups } from "@sample/backup-os";
import { Installer, installLater } from "@sample/installer";
import { systemClock } from "clock";

// Implements Backups without saying so.
class MemoryBackups {
  private entries: string[] = [];

  write(label: string): string {
    this.entries.push(label);
    return label;
  }

  prune(keep: number): void {
    this.entries.splice(0, this.entries.length - keep);
  }
}

export function main(dryRun: boolean): string {
  const os = new OsBackups();
  os.prune(3);
  const backups: Backups = dryRun ? new MemoryBackups() : os;
  const installer = new Installer(backups);
  console.log(systemClock.now());
  const later = installLater({ backups: os, name: "later" });
  later();
  const [spare] = [os];
  return installer.install("tools");
}
