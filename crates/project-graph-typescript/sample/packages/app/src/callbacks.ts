import type { Backups } from "@sample/backup-api";
import { OsBackups } from "@sample/backup-os";

// Hands a backups object to whatever function it is given.
function withBackups(use: (backups: Backups) => string): string {
  return use(new OsBackups());
}

function label(backups: Backups): string {
  return backups.write("named");
}

export function useCallbacks(): string {
  const inline = withBackups((backups) => backups.write("inline"));
  return inline + withBackups(label);
}
