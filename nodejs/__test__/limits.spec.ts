/**
 * Resource-limit and cyclic-structure tests (#336, #337)
 */

import { describe, expect, it } from 'vitest';
import { safeDump, safeDumpAll, safeLoad, safeLoadAll } from '../index';

const DEEP = `${'- '.repeat(20_000)}x`;

const BOMB = Array.from({ length: 9 }, (_, i) =>
  i === 0
    ? 'a0: &a0 [x,x,x,x,x,x,x,x,x]'
    : `a${i}: &a${i} [${Array(9)
        .fill(`*a${i - 1}`)
        .join(',')}]`
).join('\n');

const STRBOMB = `a0: &a0 "${'x'.repeat(1024)}"\n${Array.from(
  { length: 6 },
  (_, i) => `a${i + 1}: &a${i + 1} [${Array(9).fill(`*a${i}`).join(',')}]`
).join('\n')}`;

const TAGBOMB = `a0: &a0 !<tag:${'x'.repeat(10_000)}> ""\n${Array.from(
  { length: 5 },
  (_, i) => `a${i + 1}: &a${i + 1} [${Array(9).fill(`*a${i}`).join(',')}]`
).join('\n')}`;

const TAGPREFIX = `%TAG !e! tag:e.com,${'a'.repeat(100_000)}\n---\n${Array.from(
  { length: 1_000 },
  (_, i) => `k${i}: !e!x v`
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
    ['tag prefix amplification', TAGPREFIX],
  ])('safeLoad rejects %s', (_name, input) => {
    expect(() => safeLoad(input)).toThrow(/limit exceeded/);
  });

  it.each([
    ['deep input', DEEP],
    ['alias bomb', BOMB],
    ['long-scalar alias bomb', STRBOMB],
    ['long-tag alias bomb', TAGBOMB],
    ['tag prefix amplification', TAGPREFIX],
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
  it('rejects depth 257', () => {
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

  it('round-trips a 100-deep object', () => {
    let data: Record<string, unknown> = { leaf: 'x' };
    for (let i = 0; i < 99; i++) {
      data = { k: data };
    }
    expect(safeLoad(safeDump(data))).toEqual(data);
  });

  it.each([
    ['function', () => 1],
    ['symbol', Symbol('s')],
    ['bigint', 1n],
  ])('safeDump throws for %s', (_name, value) => {
    expect(() => safeDump(value)).toThrow(/cannot serialize/);
    expect(() => safeDumpAll([value])).toThrow(/cannot serialize/);
  });

  it('safeDump throws on a shared-reference expansion bomb', () => {
    let a: unknown[] = ['x'];
    for (let i = 0; i < 30; i++) {
      a = [a, a];
    }
    expect(() => safeDump(a)).toThrow(/dump node count exceeds/);
  }, 120_000);

  it('shares the output budget across safeDumpAll documents', () => {
    const limit = 100 * 1024 * 1024;
    const doc = 'a'.repeat(limit / 2);
    expect(() => safeDumpAll([doc])).not.toThrow();
    expect(() => safeDumpAll([doc, doc, doc])).toThrow(/output size exceeds/);
  }, 60_000);

  it('safeDump throws on a sparse huge array before allocating', () => {
    expect(() => safeDump(new Array(4_000_000_000))).toThrow(/dump node count exceeds/);
  });

  it('dumps small shared references', () => {
    const shared = [1, 2];
    expect(safeLoad(safeDump({ a: shared, b: shared }))).toEqual({ a: [1, 2], b: [1, 2] });
  });

  it('round-trips a legitimate alias document', () => {
    const data = safeLoad('a: &x [1, 2]\nb: *x\n');
    expect(safeLoad(safeDump(data))).toEqual(data);
  });

  it('safeDumpAll throws when output exceeds the size limit', () => {
    const text = '\u0001'.repeat(30_000_000);
    expect(() => safeDumpAll([text])).toThrow(/output size exceeds/);
    expect(() => safeDump(text)).toThrow(/output size exceeds/);
  }, 60_000);

  it('dumps a 1.1M-element list', () => {
    const list = Array.from({ length: 1_100_000 }, (_, i) => i);
    expect(safeLoad(safeDump(list))).toEqual(list);
  }, 60_000);

  it('dumps a 600k-key object', () => {
    const obj: Record<string, number> = {};
    for (let i = 0; i < 600_000; i++) {
      obj[`k${i}`] = i;
    }
    expect(Object.keys(safeLoad(safeDump(obj)) as object)).toHaveLength(600_000);
  }, 60_000);

  it('safeDumpAll dumps 120k small documents', () => {
    const docs = Array.from({ length: 120_000 }, (_, i) => ({ a: i, b: 'x', c: true, d: null }));
    expect(safeDumpAll(docs).length).toBeGreaterThan(1_000_000);
  }, 60_000);

  it('accepts output exactly at the limit and rejects one byte over', () => {
    const limit = 100 * 1024 * 1024;
    expect(safeDump('a'.repeat(limit - 1))).toHaveLength(limit);
    expect(() => safeDump('a'.repeat(limit))).toThrow(/output size exceeds/);
  }, 60_000);
});
