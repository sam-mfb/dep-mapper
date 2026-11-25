export type Label = string;

export interface Backups {
  write(label: Label): string;
  prune(keep: number): void;
}
