import { Paths } from "@sample/backup-os";

export function where(): string {
  return Paths.folder("app") + new Paths.Folder("tools").path() + Paths.Limits.Disk.quota;
}

export function defaultWrite(): string {
  return Paths.defaultBackups.write("default");
}
