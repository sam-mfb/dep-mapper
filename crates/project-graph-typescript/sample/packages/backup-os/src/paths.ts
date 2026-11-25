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

  export namespace Limits.Disk {
    export const quota = 10;
  }
}
