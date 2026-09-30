/**
 * Resource-limit and cyclic-structure tests (#336, #337)
 */

import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  formatFiles,
  formatFilesInPlace,
  Linter,
  lint,
  load,
  loadAll,
  parseParallel,
  parseParallelAsync,
  processFiles,
  safeDump,
  safeDumpAll,
  safeLoad,
  safeLoadAll,
} from '../index';

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

const seq = (depth: number): string => `${'- '.repeat(depth)}x`;

const aliasBomb = (levels: number, width: number): string =>
  Array.from({ length: levels }, (_, i) =>
    i === 0
      ? `a0: &a0 [${Array(width).fill('x').join(',')}]`
      : `a${i}: &a${i} [${Array(width)
          .fill(`*a${i - 1}`)
          .join(',')}]`
  ).join('\n');

const RAISED_BOMB = aliasBomb(7, 8);
const MAX_ALIAS_BYTES = 1_073_741_824;
const MAX_INPUT_BYTES = 1_073_741_824;

describe('Configurable parse limits', () => {
  it('keeps default limits when options are absent', () => {
    expect(() => safeLoad(seq(300))).toThrow(/nesting depth exceeds 256/);
    expect(() => safeLoad(RAISED_BOMB)).toThrow(/alias expansion exceeds 67108864/);
  });

  it('raises maxDepth where the default fails', () => {
    expect(() => safeLoad(seq(300), { maxDepth: 400 })).not.toThrow();
    expect(() => safeLoadAll(seq(300), { maxDepth: 400 })).not.toThrow();
    expect(() => load(seq(300), { maxDepth: 400 })).not.toThrow();
    expect(() => loadAll(seq(300), { maxDepth: 400 })).not.toThrow();
  });

  it('lowers maxDepth', () => {
    expect(() => safeLoad(seq(5), { maxDepth: 3 })).toThrow(/nesting depth exceeds 3/);
    expect(() => safeLoadAll(seq(5), { maxDepth: 3 })).toThrow(/nesting depth exceeds 3/);
  });

  it('accepts the maximum depth', () => {
    expect(() => safeLoad(seq(500), { maxDepth: 512 })).not.toThrow();
  });

  it('raises and lowers maxAliasBytes', () => {
    expect(() => safeLoad(RAISED_BOMB, { maxAliasBytes: MAX_ALIAS_BYTES })).not.toThrow();
    expect(() => safeLoad(aliasBomb(2, 3), { maxAliasBytes: 100 })).toThrow(
      /alias expansion exceeds 100/
    );
  });

  it.each([
    ['0', 0],
    ['-1', -1],
    ['1.5', 1.5],
    ['NaN', Number.NaN],
    ['too large', 513],
  ])('rejects maxDepth %s', (_name, value) => {
    const message = /maxDepth must be between 1 and 512, got /;
    expect(() => safeLoad('a: 1', { maxDepth: value })).toThrow(message);
    expect(() => safeLoadAll('a: 1', { maxDepth: value })).toThrow(message);
    expect(() => load('a: 1', { maxDepth: value })).toThrow(message);
    expect(() => parseParallel('a: 1', { maxDepth: value })).toThrow(message);
    expect(() => lint('a: 1', { maxDepth: value })).toThrow(message);
    expect(() => new Linter({ maxDepth: value })).toThrow(message);
    expect(() => processFiles([], { maxDepth: value })).toThrow(message);
  });

  it.each([
    ['0', 0],
    ['-1', -1],
    ['2.5', 2.5],
    ['NaN', Number.NaN],
    ['too large', MAX_ALIAS_BYTES + 1],
  ])('rejects maxAliasBytes %s', (_name, value) => {
    const message = /maxAliasBytes must be between 1 and 1073741824, got /;
    expect(() => safeLoad('a: 1', { maxAliasBytes: value })).toThrow(message);
    expect(() => safeLoadAll('a: 1', { maxAliasBytes: value })).toThrow(message);
    expect(() => parseParallel('a: 1', { maxAliasBytes: value })).toThrow(message);
    expect(() => lint('a: 1', { maxAliasBytes: value })).toThrow(message);
    expect(() => processFiles([], { maxAliasBytes: value })).toThrow(message);
  });

  it.each([
    ['0', 0],
    ['-1', -1],
    ['2.5', 2.5],
    ['NaN', Number.NaN],
    ['too large', MAX_INPUT_BYTES + 1],
  ])('rejects maxInputBytes %s', (_name, value) => {
    const message = /maxInputBytes must be between 1 and 1073741824, got /;
    expect(() => lint('a: 1', { maxInputBytes: value })).toThrow(message);
    expect(() => new Linter({ maxInputBytes: value })).toThrow(message);
  });

  it('enforces maxInputBytes in the linter by UTF-8 byte count', () => {
    const tooLarge = /input size \d+ bytes exceeds maximum allowed 16 bytes/;
    const source = `a: ${'x'.repeat(12)}\n`;
    expect(() => lint(source, { maxInputBytes: 16 })).not.toThrow();
    expect(() => lint(`${source}#`, { maxInputBytes: 16 })).toThrow(tooLarge);
    expect(() => new Linter({ maxInputBytes: 16 }).lint(`${source}#`)).toThrow(tooLarge);
    const multibyte = `a: ${'\u00e9'.repeat(7)}\n`;
    expect(multibyte.length).toBeLessThan(16);
    expect(() => lint(multibyte, { maxInputBytes: 16 })).toThrow(tooLarge);
  });

  it('accepts the bounds 1 and MAX for maxInputBytes', () => {
    expect(() => new Linter({ maxInputBytes: 1 })).not.toThrow();
    expect(() => new Linter({ maxInputBytes: MAX_INPUT_BYTES })).not.toThrow();
    expect(() => lint('a: 1', { maxInputBytes: MAX_INPUT_BYTES })).not.toThrow();
    expect(() => lint('', { maxInputBytes: 1 })).not.toThrow();
  });

  it('applies limits to parseParallel and parseParallelAsync', async () => {
    const deep = seq(300);
    expect(() => parseParallel(deep)).toThrow(/nesting depth exceeds 256/);
    expect(() => parseParallel(deep, { maxDepth: 400 })).not.toThrow();
    await expect(parseParallelAsync(deep)).rejects.toThrow(/nesting depth exceeds 256/);
    await expect(parseParallelAsync(deep, { maxDepth: 400 })).resolves.toBeDefined();
    await expect(parseParallelAsync('a: 1', { maxDepth: 0 })).rejects.toThrow(/maxDepth/);
  });

  it('applies limits to the linter', () => {
    const deep = seq(300);
    expect(() => lint(deep)).toThrow(/Linting failed/);
    expect(() => lint(deep, { maxDepth: 400 })).not.toThrow();
    expect(() => new Linter({ maxDepth: 400 }).lint(deep)).not.toThrow();
  });

  it('applies limits to batch processing', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'limits-batch-'));
    try {
      const file = path.join(dir, 'deep.yaml');
      fs.writeFileSync(file, `${seq(300)}\n`);
      const failed = processFiles([file]);
      expect(failed.failed).toBe(1);
      expect(failed.errors[0].message).toMatch(/nesting depth exceeds 256/);
      expect(processFiles([file], { maxDepth: 400 }).failed).toBe(0);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('accepts the lower bound 1 for both limits', () => {
    expect(safeLoad('a: 1', { maxDepth: 1, maxAliasBytes: 1 })).toEqual({ a: 1 });
    expect(() => parseParallel('a: 1', { maxDepth: 1, maxAliasBytes: 1 })).not.toThrow();
    expect(() => lint('a: 1', { maxDepth: 1, maxAliasBytes: 1 })).not.toThrow();
  });

  it('renders extreme values compactly', () => {
    expect(() => safeLoad('a: 1', { maxDepth: 1e300 })).toThrow(
      'maxDepth must be between 1 and 512, got 1e300'
    );
    expect(() => safeLoad('a: 1', { maxDepth: Number.POSITIVE_INFINITY })).toThrow(
      'maxDepth must be between 1 and 512, got Infinity'
    );
    expect(() => safeLoad('a: 1', { maxDepth: Number.NaN })).toThrow(
      'maxDepth must be between 1 and 512, got NaN'
    );
  });

  it('throws the bare reason from parseParallel without a code prefix', () => {
    expect(() => parseParallel('a: 1', { maxDepth: 0 })).toThrow(
      /^maxDepth must be between 1 and 512, got 0$/
    );
  });
});

describe('Batch config validation', () => {
  it.each([
    ['processFiles', processFiles],
    ['formatFiles', formatFiles],
    ['formatFilesInPlace', formatFilesInPlace],
  ])('%s throws for workers above the maximum', (_name, fn) => {
    expect(() => fn([], { workers: 129 })).toThrow(/workers must be between 0 and 128, got 129/);
  });

  it.each([
    ['formatFiles', formatFiles],
    ['formatFilesInPlace', formatFilesInPlace],
  ])('%s validates the limits', (_name, fn) => {
    expect(() => fn([], { maxDepth: 0 })).toThrow(/maxDepth must be between 1 and 512, got 0/);
    expect(() => fn([], { maxAliasBytes: -1 })).toThrow(/maxAliasBytes must be between 1/);
  });

  it('formatFiles ignores valid limits', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'limits-format-'));
    try {
      const file = path.join(dir, 'nested.yaml');
      fs.writeFileSync(file, `${seq(50)}\n`);
      const [result] = formatFiles([file], { maxDepth: 1, maxAliasBytes: 1 });
      expect(result.error).toBeFalsy();
      expect(result.content).toBeTruthy();
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
