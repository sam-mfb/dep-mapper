// Exact reachability with one stack of field/payload selectors. Tabulate a
// balanced route from each encountered entry; a matching pop resumes all
// callers waiting for that particular entry and symbol. Cycles add no new
// work after the finite sets of entry/vertex pairs and exits stop growing.
export type Edge<T> = { to: number; step: T; push?: string; pop?: string };
type Proof<T> = { prior?: Proof<T>; arc?: Edge<T>; inside?: Proof<T>; close?: Edge<T> };
export function balanced<T>(graph: Edge<T>[][], start: number, allowed: Set<number>) {
  type Entry = {
    seen: Map<number, Proof<T>>;
    wait: Map<string, Map<Entry, Proof<T>>>;
    exits: Map<string, Map<number, { inside: Proof<T>; close: Edge<T> }>>;
  };
  const entries = new Map<number, Entry>();
  const todo: { entry: Entry; v: number; proof: Proof<T> }[] = [];
  function discover(entry: Entry, v: number, proof: Proof<T>) {
    if (entry.seen.has(v)) return;
    entry.seen.set(v, proof);
    todo.push({ entry, v, proof });
  }
  function at(v: number) {
    let e = entries.get(v);
    if (!e) {
      e = { seen: new Map(), wait: new Map(), exits: new Map() };
      entries.set(v, e);
      discover(e, v, {});
    }
    return e;
  }
  const root = at(start);
  for (let i = 0; i < todo.length; i++) {
    const { entry, v, proof } = todo[i];
    for (const arc of graph[v]) {
      if (!allowed.has(arc.to)) continue;
      if (arc.push !== undefined) {
        const inner = at(arc.to);
        if (!inner.wait.has(arc.push)) inner.wait.set(arc.push, new Map());
        const waits = inner.wait.get(arc.push)!;
        if (waits.has(entry)) continue;
        const prefix = { prior: proof, arc };
        waits.set(entry, prefix);
        for (const [dest, tail] of inner.exits.get(arc.push) ?? [])
          discover(entry, dest, { prior: prefix, ...tail });
      } else if (arc.pop !== undefined) {
        if (!entry.exits.has(arc.pop)) entry.exits.set(arc.pop, new Map());
        const exits = entry.exits.get(arc.pop)!;
        if (exits.has(arc.to)) continue;
        const tail = { inside: proof, close: arc };
        exits.set(arc.to, tail);
        for (const [outer, prefix] of entry.wait.get(arc.pop) ?? [])
          discover(outer, arc.to, { prior: prefix, ...tail });
      } else discover(entry, arc.to, { prior: proof, arc });
    }
  }
  return root.seen;
}
export function explanation<T>(p: Proof<T>): T[] {
  const items: (Proof<T> | Edge<T>)[] = [p],
    out: T[] = [];
  while (items.length) {
    const next = items.pop()!;
    if ('step' in next) {
      out.push(next.step);
      continue;
    }
    if (next.close) items.push(next.close);
    if (next.inside) items.push(next.inside);
    if (next.arc) items.push(next.arc);
    if (next.prior) items.push(next.prior);
  }
  return out;
}
