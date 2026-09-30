/**
 * Resource-limit and cyclic-structure tests (#336, #337)
 */

import { describe, expect, it } from 'vitest';
import { safeDump, safeDumpAll, safeLoad, safeLoadAll } from '../index';

const DEEP = `${'- '.repeat(20_000)}x`;

const BOMB = Array.from({ length: 9 }, (_, i) =>
  i === 0
    ? 'a0: &a0 [x,x,x,x,x,x,x,x,x]'
    : `a${i}: &a${i} [${Array(9).fill(`*a${i - 1}`).join(',')}]`,
).join('\n');

const STRBOMB = `a0: &a0 "${'x'.repeat(1024)}"\n${Array.from(
  { length: 6 },
  (_, i) => `a${i + 1}: &a${i + 1} [${Array(9).fill(`*a${i}`).join(',')}]`,
).join('\n')}`;

const TAGBOMB = `a0: &a0 !<tag:${'x'.repeat(10_000)}> ""\n${Array.from(
  { length: 5 },
  (_, i) => `a${i + 1}: &a${i + 1} [${Array(9).fill(`*a${i}`).join(',')}]`,
).join('\n')}`;

const nested = (depth: number): unknown[] => {
  let data: unknown[] = [];
  for (let i = 1; i < depth; i++) {
    data = [data];
  }
  return data;
};

describe('Resource limits - load', () => {
  it.each([
    ['deep input', DEEP],
    ['alias bomb', BOMB],
    ['long-scalar alias bomb', STRBOMB],
    ['long-tag alias bomb', TAGBOMB],
  ])('safeLoad rejects %s', (_name, input) => {
    expect(() => safeLoad(input)).toThrow(/limit exceeded/);
  });

  it.each([
    ['deep input', DEEP],
    ['alias bomb', BOMB],
    ['long-scalar alias bomb', STRBOMB],
    ['long-tag alias bomb', TAGBOMB],
  ])('safeLoadAll rejects %s', (_name, input) => {
    expect(() => safeLoadAll(input)).toThrow(/limit exceeded/);
  });
});

describe('Resource limits - aliases', () => {
  it('rejects a cross-document alias', () => {
    expect(() => safeLoadAll('--- &a [x]\n--- *a\n')).toThrow(/unknown anchor/);
  });

  it('still loads shared aliases', () => {
    expect(safeLoad('a: &x [1, 2]\nb: *x\n')).toEqual({ a: [1, 2], b: [1, 2] });
  });
});

describe('Resource limits - dump', () => {
  it('accepts depth 256 and rejects depth 257', () => {
    expect(safeLoad(safeDump(nested(256)))).toEqual(nested(256));
    expect(() => safeDump(nested(257))).toThrow(/circular reference/);
  });

  it('safeDump rejects a self-referential array', () => {
    const a: unknown[] = [];
    a.push(a);
    expect(() => safeDump(a)).toThrow(/circular reference/);
  });

  it('safeDump rejects a self-referential object', () => {
    const o: Record<string, unknown> = {};
    o.x = o;
    expect(() => safeDump(o)).toThrow(/circular reference/);
  });

  it('safeDumpAll rejects self-referential documents', () => {
    const a: unknown[] = [];
    a.push(a);
    const o: Record<string, unknown> = {};
    o.x = o;
    expect(() => safeDumpAll([a])).toThrow(/circular reference/);
    expect(() => safeDumpAll([o])).toThrow(/circular reference/);
  });

  it('round-trips a 200-deep object', () => {
    let data: Record<string, unknown> = { leaf: 'x' };
    for (let i = 0; i < 199; i++) {
      data = { k: data };
    }
    expect(safeLoad(safeDump(data))).toEqual(data);
  });
});
