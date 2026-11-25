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

interface Choice {
  backups?: Backups;
}

// The default runs only when the caller leaves `backups` out.
function pick({ backups = new MemoryBackups() }: Choice): Backups {
  return backups;
}

export function main(dryRun: boolean): string {
  const os = new OsBackups();
  os.prune(3);
  const backups: Backups = dryRun ? new MemoryBackups() : os;
  const installer = new Installer(backups);
  console.log(systemClock.now());
  const later = installLater({ backups: os, name: "later" });
  later();
  // The literal supplies the element, so the default never runs.
  const [spare = new MemoryBackups()] = [os];
  spare.write("spare");
  pick({}).prune(1);
  return installer.install("tools");
}
