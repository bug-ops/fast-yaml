/**
 * Set and Map handling: dump maps them, load keeps the js-yaml object form, positions are reported.
 */

import { runInNewContext } from 'node:vm';
import { describe, expect, it } from 'vitest';
import { parseParallel, safeDump, safeDumpAll, safeLoad, safeLoadAll } from '../index';

describe('safeDump of Set and Map', () => {
  it('dumps a Set as a !!set', () => {
    expect(safeDump({ s: new Set(['a', 'b']) })).toBe('s: !!set\n  a: ~\n  b: ~\n');
  });

  it('dumps a Map as a mapping with any key type', () => {
    const map = new Map<unknown, unknown>([
      ['a', 1],
      [2, 'two'],
    ]);
    expect(safeDump({ m: map })).toBe('m:\n  a: 1\n  2: two\n');
  });

  it('dumps nested and root collections', () => {
    expect(safeDump(new Set([1, 2]))).toContain('1');
    expect(safeDump(new Map([['k', new Set(['x'])]]))).toBe('k: !!set\n  x: ~\n');
    expect(safeDump(new Map([['k', new Map([['n', [1, 2]]])]]))).toBe(
      'k:\n  n:\n    - 1\n    - 2\n'
    );
    expect(safeDumpAll([new Set(['a']), { b: new Map([['c', 1]]) }])).toContain('---');
  });

  it('keeps the members when the dump is loaded back', () => {
    const dumped = safeDump({ s: new Set(['a', 'b']) });
    expect(safeLoad(dumped)).toEqual({ s: { a: null, b: null } });
  });

  it('dumps an empty Set and Map', () => {
    expect(safeDump({ s: new Set(), m: new Map() })).toBe('s: !!set {}\nm: {}\n');
  });

  it('recognises a Set from another realm', () => {
    const foreign = runInNewContext('({ s: new Set(["a"]), m: new Map([["k", 1]]) })');
    expect(foreign.s instanceof Set).toBe(false);
    expect(safeDump(foreign)).toBe('s: !!set\n  a: ~\nm:\n  k: 1\n');
  });

  it('rejects members that are the same YAML value', () => {
    expect(() => safeDump(new Set([null, undefined]))).toThrow(/two Set members/);
    const nullKeys = new Map<unknown, unknown>([
      [null, 1],
      [undefined, 2],
    ]);
    expect(() => safeDump(nullKeys)).toThrow(/two Map keys/);
  });

  it('throws a JS error, not a panic, for an object that only claims the tag', () => {
    expect(() => safeDump({ [Symbol.toStringTag]: 'Set' })).toThrow(/incompatible receiver/);
    expect(() => safeDump({ [Symbol.toStringTag]: 'Map', size: 2 })).toThrow(
      /incompatible receiver/
    );
    expect(() => safeDump(new Map([[1, 2]]), {})).not.toThrow();
    const mapClaimingSet = Object.defineProperty(new Map([[1, 2]]), Symbol.toStringTag, {
      value: 'Set',
    });
    expect(() => safeDump(mapClaimingSet)).toThrow(/incompatible receiver/);
  });

  it('never runs a hostile size getter, length or iterator of a tagged object', () => {
    let touched = false;
    const touch = () => {
      touched = true;
      return 1;
    };
    const hostile = {
      [Symbol.toStringTag]: 'Set',
      get size(): number {
        return touch();
      },
      get length(): number {
        return touch();
      },
      *[Symbol.iterator]() {
        touched = true;
        yield 1;
      },
    };
    expect(() => safeDump(hostile)).toThrow(/incompatible receiver/);
    expect(touched).toBe(false);
  });

  it('does not materialize an array-like or infinite tagged object', () => {
    const arrayLike = { [Symbol.toStringTag]: 'Set', size: 0, length: 2 ** 32 - 1 };
    expect(() => safeDump(arrayLike)).toThrow(/incompatible receiver/);
    const infinite = {
      [Symbol.toStringTag]: 'Set',
      size: 1,
      *[Symbol.iterator]() {
        for (;;) yield 1;
      },
    };
    expect(() => safeDump(infinite)).toThrow(/incompatible receiver/);
    expect(() => safeDump({ [Symbol.toStringTag]: 'Map', size: 1, length: 2 ** 32 - 1 })).toThrow(
      /incompatible receiver/
    );
  });

  it('reads a real Set through the built-ins whatever it overrides', () => {
    const set = new Set([1, 2]);
    Object.defineProperty(set, 'size', { value: 0 });
    Object.defineProperty(set, Symbol.iterator, {
      value: function* () {
        for (;;) yield 9;
      },
    });
    expect(safeDump(set)).toBe('!!set\n1: ~\n2: ~\n');
  });

  it('does not treat a plain object with another tag as a collection', () => {
    expect(safeDump({ [Symbol.toStringTag]: 'Other', a: 1 })).toBe('a: 1\n');
    expect(safeDump(Object.create(null))).toBe('{}\n');
  });

  it('writes a Map whose keys 1 and "1" are valid YAML that safeLoad then rejects', () => {
    const dumped = safeDump(
      new Map<unknown, unknown>([
        [1, 'a'],
        ['1', 'b'],
      ])
    );
    expect(dumped).toContain('"1"');
    expect(() => safeLoad(dumped)).toThrow(/distinct in YAML.*line 2, column 1/);
  });

  it('writes huge and small numbers so YAML 1.1 readers read them as floats', () => {
    expect(safeDump({ a: 1e300, b: 1.5e-7, c: -0.5 })).toBe('a: 1.0e+300\nb: 1.5e-7\nc: -0.5\n');
    expect(safeLoad(safeDump({ a: 1e300 }))).toEqual({ a: 1e300 });
  });

  it('dumps a large Set', () => {
    const large = new Set<number>();
    for (let i = 0; i < 20000; i++) large.add(i);
    expect(safeDump(large).length).toBeGreaterThan(0);
  });
});

describe('loading !!set', () => {
  it('keeps the js-yaml object form with null members', () => {
    const doc = safeLoad('s: !!set {a, b}');
    expect(doc).toEqual({ s: { a: null, b: null } });
    expect(JSON.stringify(doc)).toBe('{"s":{"a":null,"b":null}}');
  });

  it('rejects a member with a value, at its position', () => {
    expect(() => safeLoad('s: !!set {a: 1}')).toThrow(
      /member has a non-null value.*line 1, column 11/
    );
    expect(() => safeLoadAll('a: 1\n---\n!!set\n  x: 1\n')).toThrow(
      /line 4, column 3 \(document 2\)/
    );
    expect(() => parseParallel('!!set {a: 1}')).toThrow(/member has a non-null value/);
  });

  it('reports colliding set members with a position', () => {
    expect(() => safeLoad("!!set {1, '1'}")).toThrow(/line 1, column 11/);
  });
});

describe('repeated merge keys', () => {
  it('rejects a repeated << with its position', () => {
    expect(() => safeLoad('a: &a {x: 1}\nb: &b {y: 2}\nc: {<<: *a, <<: *b}\n')).toThrow(
      /duplicate merge key.*line 3, column 13/
    );
  });
});

describe('keys that share a property name in parseParallel', () => {
  it('reports the later key with its position', () => {
    expect(() => parseParallel("a: 1\n---\n1: x\n'1': y\n")).toThrow(
      /line 4, column 1 \(document 2\)/
    );
  });
});

describe('flow nesting', () => {
  const flow = (depth: number) => `${'['.repeat(depth)}1${']'.repeat(depth)}`;

  it('accepts 255 flow levels and reports the cap beyond', () => {
    expect(safeLoad(flow(255))).toBeDefined();
    expect(() => safeLoad(flow(256), { maxDepth: 512 })).toThrow(
      /flow collection nesting exceeds the scanner limit of 255/
    );
  });
});
