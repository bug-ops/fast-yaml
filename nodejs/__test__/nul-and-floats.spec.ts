/**
 * Regression tests for NUL input (#417) and leading-dot / signed-infinity floats (#393)
 */

import { describe, expect, it } from 'vitest';
import { safeDump, safeLoad, safeLoadAll } from '../index';

describe('NUL input', () => {
  it('should reject NUL instead of truncating the document', () => {
    for (const text of ['a: 1\0\nb: 2\n', '\0a: 1\n', '# c\0\na: 1\n', 'a: "x\0y"\n']) {
      expect(() => safeLoad(text)).toThrow(/NUL/);
      expect(() => safeLoadAll(text)).toThrow(/NUL/);
    }
  });
});

describe('float-like scalars', () => {
  it('should resolve leading-dot and signed-infinity floats', () => {
    expect(safeLoad('[.5, -.5, +.inf, +.nan]')).toEqual([0.5, -0.5, Infinity, '+.nan']);
  });

  it('should round-trip float-like strings through dump', () => {
    for (const text of ['+.inf', '+.Inf', '+.INF', '.5', '-.5']) {
      for (const data of [{ k: text }, { [text]: 1 }, [text]]) {
        expect(safeLoad(safeDump(data))).toEqual(data);
      }
    }
  });
});

describe('negative zero and BigInt (#557)', () => {
  it('dumps -0 as a float that loads back as -0', () => {
    expect(safeDump(-0)).toBe('-0.0\n');
    expect(safeDump({ a: -0, b: 0 })).toBe('a: -0.0\nb: 0\n');
    expect(Object.is(safeLoad(safeDump(-0)), -0)).toBe(true);
    expect(Object.is(safeLoad('a: -0.0\n').a, -0)).toBe(true);
  });

  it('dumps a BigInt as a YAML integer', () => {
    expect(safeDump(5n)).toBe('5\n');
    expect(safeDump(-(2n ** 63n))).toBe('-9223372036854775808\n');
    expect(safeDump({ k: 2n ** 70n })).toBe('k: 1180591620717411303424\n');
    expect(safeDump([-(2n ** 70n)], { defaultFlowStyle: true })).toBe('[-1180591620717411303424]\n');
    expect(safeDump(10n ** 400n)).toBe(`1${'0'.repeat(400)}\n`);
  });

  it('loads integers beyond i64 as decimal strings', () => {
    expect(safeLoad('18446744073709551616')).toBe('18446744073709551616');
    expect(safeLoad(safeDump(2n ** 70n))).toBe('1180591620717411303424');
  });

  it('escapes U+0085, U+2028 and U+2029 in dumps', () => {
    expect(safeDump('a\u0085b')).toBe('"a\\x85b"\n');
    expect(safeDump({ k: 'a b c' })).toBe('k: "a\\u2028b\\u2029c"\n');
  });

  it('loads an empty or comment-only file as null', () => {
    expect(safeLoad('# only a comment\n')).toBeNull();
    expect(safeLoad('')).toBeNull();
    expect(safeLoadAll('')).toEqual([]);
  });

  it('writes long flow keys explicitly and quotes ? in flow scalars', () => {
    const long = 'k'.repeat(1100);
    const flow = safeDump({ [long]: 1 }, { defaultFlowStyle: true });
    expect(flow.startsWith('{? kkk')).toBe(true);
    expect(safeLoad(flow)).toEqual({ [long]: 1 });
    expect(safeDump({ a: ['x?y'] }, { defaultFlowStyle: true })).toBe('{a: ["x?y"]}\n');
  });
});
