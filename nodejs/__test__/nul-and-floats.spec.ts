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
