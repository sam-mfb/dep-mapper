import type { Backups } from "@sample/backup-api";
import { OsBackups } from "./os.ts";

export namespace Paths {
  export function folder(name: string): string {
    return `${root()}/${name}`;
  }

  function root(): string {
    return "/backups";
  }

  export class Folder {
    constructor(readonly name: string) {}

    path(): string {
      return folder(this.name);
    }
  }

  export const defaultBackups: Backups = new OsBackups();

  export namespace Limits.Disk {
    export const quota = 10;
  }
}

// A class and a namespace of the same name merge into one item.
export class Shelf {
  size(): number {
    return Shelf.slots;
  }
}

export namespace Shelf {
  export const slots = 4;
  function label(): string {
    return "shelf";
  }
}
