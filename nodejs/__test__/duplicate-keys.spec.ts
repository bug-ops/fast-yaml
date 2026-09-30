/**
 * Duplicate mapping keys keep the first key position and the last value (#522)
 */

import { describe, expect, it } from 'vitest';
import { load, parseParallel, safeLoad } from '../index';

type Loader = (source: string) => unknown;

const loaders: [string, Loader][] = [
  ['load', (s) => load(s)],
  ['safeLoad', (s) => safeLoad(s)],
  ['parseParallel', (s) => parseParallel(s)[0]],
];

const cases: [string, string, Record<string, unknown>, string[]][] = [
  ['literal duplicate', 'a: 1\nb: 2\na: 3\n', { a: 3, b: 2 }, ['a', 'b']],
  ['three occurrences', 'a: 1\nb: 2\na: 3\nc: 4\na: 5\n', { a: 5, b: 2, c: 4 }, ['a', 'b', 'c']],
  ['nested mapping', 'o:\n  x: 1\n  y: 2\n  x: 3\n', { o: { x: 3, y: 2 } }, ['o']],
  ['flow mapping', '{a: 1, b: 2, a: 3}\n', { a: 3, b: 2 }, ['a', 'b']],
  ['anchored key with alias', '&k a: 1\nb: 2\n*k : 3\n', { a: 3, b: 2 }, ['a', 'b']],
  ['tagged key', '!!str a: 1\nb: 2\na: 3\n', { a: 3, b: 2 }, ['a', 'b']],
  ['explicit complex key', '? a\n: 1\nb: 2\n? a\n: 3\n', { a: 3, b: 2 }, ['a', 'b']],
  ['!!set', '!!set {a, b, a}\n', { a: null, b: null }, ['a', 'b']],
];

describe.each(loaders)('duplicate keys via %s', (_name, loader) => {
  it.each(cases)('%s', (_label, source, expected, order) => {
    const result = loader(source) as Record<string, unknown>;
    expect(result).toEqual(expected);
    expect(Object.keys(result)).toEqual(order);
  });
});
