import assert from 'node:assert/strict';
import { balanced, explanation, type Edge } from './balanced.ts';

const graph: Edge<string>[][] = Array.from({ length: 204 }, () => []);
for (let i = 0; i < 100; i++) {
  graph[i].push({ to: i + 1, push: String(i), step: 'push' });
  graph[100 + i].push({ to: 101 + i, pop: String(99 - i), step: 'pop' });
}
graph[0].push({ to: 0, push: 'recursive', step: 'enter' });
graph[200].push({ to: 201, pop: 'recursive', step: 'exit' });
graph[100].push({ to: 202, pop: 'mismatch', step: 'invalid' });
graph[0].push({ to: 203, pop: 'recursive', step: 'return immediately' });
const reached = balanced(graph, 0, new Set(graph.keys()));
assert.equal(reached.has(202), false);
assert.equal(reached.has(100), false);
assert.deepEqual(explanation(reached.get(200)!), [
  ...Array(100).fill('push'),
  ...Array(100).fill('pop'),
]);
assert.equal(reached.has(201), true);
assert.equal(reached.has(203), true);
console.log('PASS balanced field paths, cycles, mismatched fields, and nesting to depth 100');
