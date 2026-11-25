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

function sameBackups(backups: Backups): Backups {
  return backups;
}

// The parameter's type names a project function, so the checker resolves
// calls through it to that function; they still call whatever was given.
function withTyped(use: typeof sameBackups): Backups {
  return use(new OsBackups());
}

export function useTyped(): string {
  return withTyped((given) => {
    given.prune(5);
    return given;
  }).write("typed");
}
