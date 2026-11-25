// A third-party package, outside the workspace.
export interface Clock {
  now(): number;
}
export declare const systemClock: Clock;
export declare function now(): number;
