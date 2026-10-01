/**
 * Input hardening tests: numeric option validation (#428) and non-recursive lint rules (#423)
 */

import { describe, expect, it } from 'vitest';
import {
  formatFiles,
  formatFilesInPlace,
  Linter,
  lint,
  Mark,
  parseParallel,
  parseParallelAsync,
  processFiles,
  safeDump,
  safeDumpAll,
} from '../index';

const BAD_VALUES = [-1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 32, 2 ** 53];

type Call = (value: number) => unknown;

interface OptionCase {
  name: string;
  call: Call;
  valid: number;
  outOfRange?: number[];
}

const YAML = 'a: 1\n';
const MULTI = 'a: 1\n---\nb: 2\n';

const cases: OptionCase[] = [
  { name: 'indent', call: (v) => safeDump({ a: 1 }, { indent: v }), valid: 4 },
  {
    name: 'width',
    call: (v) => safeDump({ a: 1 }, { width: v }),
    valid: 80,
  },
  {
    name: 'indent (safeDumpAll)',
    call: (v) => safeDumpAll([{ a: 1 }], { indent: v }),
    valid: 4,
  },
  { name: 'workers', call: (v) => processFiles([], { workers: v }), valid: 2, outOfRange: [129] },
  {
    name: 'maxInputBytes (batch)',
    call: (v) => processFiles([], { maxInputBytes: v }),
    valid: 1024,
    outOfRange: [0, 1024 ** 3 + 1],
  },
  {
    name: 'sequentialThreshold',
    call: (v) => processFiles([], { sequentialThreshold: v }),
    valid: 1024,
  },
  { name: 'indent (batch)', call: (v) => formatFiles([], { indent: v }), valid: 4 },
  { name: 'width (batch)', call: (v) => formatFilesInPlace([], { width: v }), valid: 100 },
  {
    name: 'threadCount',
    call: (v) => parseParallel(MULTI, { threadCount: v }),
    valid: 2,
    outOfRange: [129],
  },
  {
    name: 'minChunkSize',
    call: (v) => parseParallel(MULTI, { minChunkSize: v }),
    valid: 1024,
    outOfRange: [0],
  },
  {
    name: 'maxInputBytes (parallel)',
    call: (v) => parseParallel(MULTI, { maxInputBytes: v }),
    valid: 1024 * 1024,
    outOfRange: [0, 1024 ** 3 + 1],
  },
  {
    name: 'maxDocuments',
    call: (v) => parseParallel(MULTI, { maxDocuments: v }),
    valid: 1000,
    outOfRange: [0, 10_000_001],
  },
  { name: 'Mark line', call: (v) => new Mark('f', v, 0), valid: 5 },
  { name: 'Mark column', call: (v) => new Mark('f', 0, v), valid: 5 },
];

describe('numeric option validation', () => {
  for (const { name, call, valid, outOfRange = [] } of cases) {
    describe(name, () => {
      it('accepts a valid value', () => {
        expect(() => call(valid)).not.toThrow();
      });

      it.each([...BAD_VALUES, ...outOfRange])('rejects %s', (bad) => {
        expect(() => call(bad)).toThrow(/must be between \d+ and \d+, got/);
      });

      it('reports InvalidArg', () => {
        try {
          call(-1);
          expect.unreachable();
        } catch (e) {
          expect((e as { code?: string }).code).toBe('InvalidArg');
        }
      });
    });
  }

  it.each([
    ['indent', { indent: 0 }],
    ['indent', { indent: 100 }],
    ['width', { width: 1 }],
    ['width', { width: 100_000 }],
  ])('rejects safeDump %s out of range (%o)', (name, opts) => {
    expect(() => safeDump({ a: 1 }, opts)).toThrow(new RegExp(`${name} must be between`));
    expect(() => safeDumpAll([{ a: 1 }], opts)).toThrow(new RegExp(`${name} must be between`));
  });

  it('accepts boundary values', () => {
    expect(() => lint(YAML, { maxLineLength: 1 })).not.toThrow();
    expect(() => lint(YAML, { maxLineLength: 1000 })).not.toThrow();
    expect(() => lint(YAML, { indentSize: 16 })).not.toThrow();
    expect(() => processFiles([], { workers: 0 })).not.toThrow();
    expect(() => new Mark('f', 4294967295, 4294967295)).not.toThrow();
  });

  it('accepts negative zero as zero', () => {
    const mark = new Mark('f', -0, -0);
    expect(mark.line).toBe(0);
    expect(mark.column).toBe(0);
    expect(() => processFiles([], { workers: -0 })).not.toThrow();
    expect(() => parseParallel(MULTI, { threadCount: -0 })).not.toThrow();
  });

  it('accepts exactly 2**32 - 1 in u32 options', () => {
    const max = 2 ** 32 - 1;
    expect(() => processFiles([], { sequentialThreshold: max })).not.toThrow();
    expect(() => formatFiles([], { indent: 9 })).not.toThrow();
    expect(() => formatFilesInPlace([], { width: 1000 })).not.toThrow();
    expect(() => parseParallel(MULTI, { minChunkSize: max })).not.toThrow();
  });

  it('enforces maxDocuments', () => {
    expect(parseParallel(MULTI, { maxDocuments: 2 })).toHaveLength(2);
    expect(() => parseParallel(MULTI, { maxDocuments: 1 })).toThrow(
      /document count exceeds 1/
    );
  });

  it('enforces maxDocuments asynchronously', async () => {
    await expect(parseParallelAsync(MULTI, { maxDocuments: 1 })).rejects.toThrow(
      /document count exceeds 1/
    );
  });

  it('rejects invalid values asynchronously', async () => {
    await expect(parseParallelAsync(MULTI, { threadCount: -1 })).rejects.toThrow(
      /threadCount must be between 0 and 128, got/
    );
  });
});

function nested(depth: number): Record<string, unknown> {
  const root: Record<string, unknown> = {};
  let cur = root;
  for (let i = 0; i < depth; i++) {
    const next: Record<string, unknown> = {};
    cur.a = next;
    cur = next;
  }
  return root;
}

describe('lint rules input hardening', () => {
  const cyclicObject = (): Record<string, unknown> => {
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    return cyclic;
  };

  it('rejects deeply nested rule values without crashing', () => {
    expect(() => lint(YAML, { rules: { 'line-length': nested(20_000) as never } })).toThrow(
      /nested deeper/
    );
  });

  it('rejects deeply nested option values without crashing', () => {
    expect(() =>
      lint(YAML, { rules: { 'line-length': { max: nested(20_000) } as never } })
    ).toThrow(/nested deeper/);
    expect(() =>
      lint(YAML, { rules: { 'line-length': { severity: nested(20_000) as never } } })
    ).toThrow();
  });

  it('rejects a cyclic rule value', () => {
    expect(() => lint(YAML, { rules: { 'line-length': cyclicObject() as never } })).toThrow(
      /nested deeper/
    );
  });

  it('rejects a cyclic value inside options', () => {
    expect(() =>
      lint(YAML, { rules: { 'line-length': { max: cyclicObject() } as never } })
    ).toThrow(/nested deeper/);
  });

  it('rejects a cyclic rules map', () => {
    expect(() => lint(YAML, { rules: cyclicObject() as never })).toThrow();
  });

  it('rejects a cyclic array', () => {
    const cyclic: unknown[] = [];
    cyclic.push(cyclic);
    expect(() => lint(YAML, { rules: { 'line-length': cyclic as never } })).toThrow(
      /nested deeper/
    );
  });

  const messageOf = (rules: unknown): string => {
    try {
      lint(YAML, { rules: rules as never });
    } catch (e) {
      return (e as Error).message;
    }
    return '';
  };

  it('accepts 99990 scalars and rejects 100001 at the node limit', () => {
    const items = (n: number) => Array.from({ length: n }, () => 1);
    expect(messageOf({ 'line-length': { max: items(99_990) } })).not.toMatch(/more than/);
    expect(messageOf({ 'line-length': { max: items(100_001) } })).toMatch(
      /more than 100000 values/
    );
  });

  it('rejects an object with too many keys', () => {
    const wide: Record<string, number> = {};
    for (let i = 0; i < 100_001; i++) wide[`k${i}`] = 1;
    expect(messageOf({ 'line-length': wide })).toMatch(/more than 100000 values/);
  });

  it('accepts depth 16 and rejects depth 17', () => {
    const wrap = (levels: number): unknown => {
      let v: unknown = 1;
      for (let i = 0; i < levels; i++) v = { a: v };
      return v;
    };
    expect(messageOf(wrap(16))).not.toMatch(/nested deeper/);
    expect(messageOf(wrap(17))).toMatch(/nested deeper than 16/);
  });

  it.each([Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY])(
    'rejects non-finite number %s',
    (n) => {
      expect(messageOf({ 'line-length': { max: n } })).toMatch(/non-finite number/);
    }
  );

  it('rejects BigInt with a clear message', () => {
    expect(messageOf({ 'line-length': { max: 120n } })).toMatch(/BigInt is not supported/);
  });

  it('rejects unsupported value types', () => {
    expect(() => lint(YAML, { rules: { 'line-length': (() => 1) as never } })).toThrow(
      /unsupported value/
    );
  });

  it('rejects deep values through Linter', () => {
    expect(() => new Linter({ rules: { x: nested(20_000) as never } })).toThrow();
  });

  it('treats null and undefined rules as no rules', () => {
    expect(() => lint(YAML, { rules: null as never })).not.toThrow();
    expect(() => lint(YAML, { rules: undefined })).not.toThrow();
    expect(() => new Linter({ rules: null as never })).not.toThrow();
  });

  it('still applies valid rule config', () => {
    const dup = 'k: 1\nk: 2\n';
    const diags = lint(dup, { rules: { 'duplicate-key': { enabled: false } } });
    expect(diags.some((d) => d.code === 'duplicate-key')).toBe(false);
  });
});

describe('removed maxInputSize option', () => {
  const renamed = /maxInputSize was renamed to maxInputBytes/;

  it('throws for ParallelConfig and BatchConfig', () => {
    expect(() => parseParallel(MULTI, { maxInputSize: 1024 })).toThrow(renamed);
    expect(() => processFiles([], { maxInputSize: 1024 })).toThrow(renamed);
    expect(() => formatFiles([], { maxInputSize: 1024 })).toThrow(renamed);
  });
});
