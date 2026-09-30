/**
 * Unit tests for YAML parser functions
 */

import { describe, expect, it } from 'vitest';
import { safeDump, safeDumpAll, safeLoad, safeLoadAll, version } from '../index';

describe('Core API - Parser', () => {
  describe('version', () => {
    it('should return a version string', () => {
      const v = version();
      expect(v).toBeTruthy();
      expect(typeof v).toBe('string');
      expect(v).toMatch(/^\d+\.\d+\.\d+/);
    });
  });

  describe('safeLoad', () => {
    it('should report complex keys with a readable message', () => {
      expect(() => safeLoad('? !!set {a}\n: 1')).toThrow(
        'YAML complex keys (sequences or mappings as keys) are not supported as JavaScript object keys'
      );
      expect(() => safeLoad('? [a, b]\n: 1')).toThrow(/complex keys/);
    });

    it('should parse simple YAML', () => {
      const result = safeLoad('name: test\nvalue: 123');
      expect(result).toEqual({ name: 'test', value: 123 });
    });

    it('should parse nested structures', () => {
      const yaml = `
person:
  name: John
  age: 30
  hobbies:
    - reading
    - coding
`;
      const result = safeLoad(yaml);
      expect(result).toEqual({
        person: {
          name: 'John',
          age: 30,
          hobbies: ['reading', 'coding'],
        },
      });
    });

    it('should handle YAML 1.2.2 booleans', () => {
      // YAML 1.2.2 Core Schema: true/True/TRUE and false/False/FALSE are all booleans
      expect(safeLoad('value: true')).toEqual({ value: true });
      expect(safeLoad('value: false')).toEqual({ value: false });
      expect(safeLoad('value: TRUE')).toEqual({ value: true });
      expect(safeLoad('value: FALSE')).toEqual({ value: false });
      expect(safeLoad('value: True')).toEqual({ value: true });
      expect(safeLoad('value: False')).toEqual({ value: false });

      // YAML 1.2.2: yes/no are strings, not booleans
      expect(safeLoad('value: yes')).toEqual({ value: 'yes' });
      expect(safeLoad('value: no')).toEqual({ value: 'no' });
      expect(safeLoad('value: on')).toEqual({ value: 'on' });
      expect(safeLoad('value: off')).toEqual({ value: 'off' });
    });

    it('should handle null values', () => {
      expect(safeLoad('value: ~')).toEqual({ value: null });
      expect(safeLoad('value: null')).toEqual({ value: null });
      expect(safeLoad('value:')).toEqual({ value: null });

      // YAML 1.2.2 Core Schema: null/Null/NULL are all null
      expect(safeLoad('value: Null')).toEqual({ value: null });
      expect(safeLoad('value: NULL')).toEqual({ value: null });
    });

    it('should handle numbers', () => {
      expect(safeLoad('int: 123')).toEqual({ int: 123 });
      expect(safeLoad('negative: -456')).toEqual({ negative: -456 });
      expect(safeLoad('float: 1.23')).toEqual({ float: 1.23 });
      expect(safeLoad('exp: 1.23e+3')).toEqual({ exp: 1230.0 });
      expect(safeLoad('hex: 0xC')).toEqual({ hex: 12 });
      expect(safeLoad('octal: 0o14')).toEqual({ octal: 12 });
    });

    it('should handle special float values', () => {
      const infResult = safeLoad('value: .inf') as { value: number };
      expect(infResult.value).toBe(Number.POSITIVE_INFINITY);

      const negInfResult = safeLoad('value: -.inf') as { value: number };
      expect(negInfResult.value).toBe(Number.NEGATIVE_INFINITY);

      const nanResult = safeLoad('value: .nan') as { value: number };
      expect(nanResult.value).toBeNaN();
    });

    it('should handle arrays', () => {
      const result = safeLoad('items:\n  - one\n  - two\n  - three');
      expect(result).toEqual({ items: ['one', 'two', 'three'] });
    });

    it('should handle empty input', () => {
      expect(safeLoad('')).toBe(null);
      expect(safeLoad('   ')).toBe(null);
    });

    it('should throw on invalid YAML', () => {
      expect(() => safeLoad('invalid: [')).toThrow(/YAML parse error/);
      expect(() => safeLoad('key: {invalid')).toThrow();
    });

    it('should enforce 100MB size limit', () => {
      // Create a string larger than 100MB (~105MB)
      const large = 'x: '.repeat(35_000_000);
      expect(() => safeLoad(large)).toThrow(/exceeds maximum/);
    });
  });

  describe('safeLoadAll', () => {
    it('should parse single document', () => {
      const docs = safeLoadAll('name: test');
      expect(docs).toHaveLength(1);
      expect(docs[0]).toEqual({ name: 'test' });
    });

    it('should parse multiple documents', () => {
      const yaml = '---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3';
      const docs = safeLoadAll(yaml);
      expect(docs).toHaveLength(3);
      expect(docs[0]).toEqual({ foo: 1 });
      expect(docs[1]).toEqual({ bar: 2 });
      expect(docs[2]).toEqual({ baz: 3 });
    });

    it('should handle empty input', () => {
      expect(safeLoadAll('')).toEqual([]);
      expect(safeLoadAll('   ')).toEqual([]);
    });

    it('should throw on invalid YAML', () => {
      expect(() => safeLoadAll('---\nvalid: true\n---\ninvalid: [')).toThrow();
    });

    it('should enforce 100MB size limit', () => {
      const large = 'x: '.repeat(35_000_000); // ~105MB, exceeds 100MB limit
      expect(() => safeLoadAll(large)).toThrow(/exceeds maximum/);
    });
  });

  describe('merge keys', () => {
    // Non-numeric keys only: JS objects reorder integer-like keys first
    it('should place merged keys before explicit keys and let explicit keys win in place', () => {
      const result = safeLoad('b: &b {x: 1, y: 2}\nm:\n  k: 0\n  <<: *b\n  y: 9\n') as {
        m: Record<string, number>;
      };
      expect(Object.entries(result.m)).toEqual([
        ['x', 1],
        ['y', 9],
        ['k', 0],
      ]);
    });

    it('should apply sequence merges in forward order with the earlier item winning', () => {
      const result = safeLoad(
        'a: &a {x: 1, p: A}\nb: &b {y: 2, p: B}\nm:\n  <<: [*a, *b]\n  k: 0\n'
      ) as {
        m: Record<string, unknown>;
      };
      expect(Object.entries(result.m)).toEqual([
        ['x', 1],
        ['p', 'A'],
        ['y', 2],
        ['k', 0],
      ]);
    });

    it.each(["'<<'", '"<<"', '!!str <<'])('should treat %s as an ordinary key', (key) => {
      const result = safeLoad(`b: &b {x: 1}\nm:\n  ${key}: *b\n  k: 0\n`) as {
        m: Record<string, unknown>;
      };
      expect(result.m).toEqual({ '<<': { x: 1 }, k: 0 });
    });

    it('should not merge a JSON "<<" key', () => {
      const result = safeLoad('{"m": {"<<": {"admin": true}, "k": 0}}') as {
        m: Record<string, unknown>;
      };
      expect(result.m).toEqual({ '<<': { admin: true }, k: 0 });
    });

    it.each(['1', 'null', '[1]', '[[{x: 1}]]', 'text', '[{x: 1}, 5]'])(
      'should reject the non-mapping merge value %s',
      (merge) => {
        expect(() => safeLoad(`m:\n  <<: ${merge}\n  k: 0\n`)).toThrow(/merge key/);
      }
    );

    it('should report the line and column of the offending << key', () => {
      expect(() => safeLoad('a: 1\nm:\n  k: 0\n  <<: 1\n')).toThrow(/at line 4, column 3/);
      expect(() => safeLoad('a: 1\n---\nb: 2\n---\nm:\n  <<: [5]\n')).toThrow(
        /at line 6, column 3/
      );
    });

    it.each(['*s', '[*s]'])('should reject a !!set merge source %s', (merge) => {
      expect(() => safeLoad(`s: &s !!set {x, y}\nm:\n  <<: ${merge}\n`)).toThrow(/merge key/);
    });

    it('should keep << as an ordinary element of a !!set, also through an alias', () => {
      const result = safeLoad('a: &a !!set {k, <<}\nb: *a\n') as Record<string, unknown>;
      expect(result.a).toEqual({ k: null, '<<': null });
      expect(result.b).toEqual({ k: null, '<<': null });
    });

    it('should accept explicitly tagged mapping and sequence merge values', () => {
      const result = safeLoad('m:\n  <<: !!seq [!!map {x: 1}, {y: 2}]\n  k: 0\n') as {
        m: Record<string, number>;
      };
      expect(result.m).toEqual({ x: 1, y: 2, k: 0 });
    });

    it.each([
      'm:\n  <<: {<<: 1}\n',
      'a: &a {<<: 1}\nm:\n  <<: *a\n',
      'm:\n  <<: [{x: 1}, {<<: [2]}]\n',
    ])('should reject a nested invalid merge value in %j', (doc) => {
      expect(() => safeLoad(doc)).toThrow(/merge key/);
    });

    it.each([
      'm:\n  <<: 1\n  <<: {a: 1}\n',
      's: &s !!set {x}\nm:\n  <<: *s\n  <<: {a: 1}\n',
      'm: {<<: [2], <<: {a: 1}}\n',
    ])('should reject an invalid earlier duplicate << value in %j', (doc) => {
      expect(() => safeLoad(doc)).toThrow(/merge key/);
    });

    it('should keep only the last of duplicate plain << keys', () => {
      const result = safeLoad('a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n') as {
        m: Record<string, number>;
      };
      expect(result.m).toEqual({ y: 2 });
    });

    it('should merge through an alias to a plain << key scalar', () => {
      const result = safeLoad('k: &k <<\nb: &b {x: 1}\nm:\n  *k : *b\n  z: 0\n') as {
        m: Record<string, number>;
      };
      expect(result.m).toEqual({ x: 1, z: 0 });
    });

    it('should round-trip a string << key through safeDump in block and flow style', () => {
      const data = { m: { '<<': { admin: true }, k: 0 }, n: { '<<': 1 } };
      for (const defaultFlowStyle of [true, false]) {
        expect(safeLoad(safeDump(data, { defaultFlowStyle }))).toEqual(data);
      }
    });
  });
});

describe('Core API - Serializer', () => {
  describe('safeDump', () => {
    it('should serialize simple objects', () => {
      const yaml = safeDump({ name: 'test', value: 123 });
      expect(yaml).toContain('name: test');
      expect(yaml).toContain('value: 123');
    });

    it('should serialize nested structures', () => {
      const data = {
        person: {
          name: 'John',
          age: 30,
        },
      };
      const yaml = safeDump(data);
      expect(yaml).toContain('person:');
      expect(yaml).toContain('name: John');
      expect(yaml).toContain('age: 30');
    });

    it('should serialize arrays', () => {
      const yaml = safeDump({ items: ['one', 'two', 'three'] });
      expect(yaml).toContain('items:');
      expect(yaml).toContain('- one');
      expect(yaml).toContain('- two');
      expect(yaml).toContain('- three');
    });

    it('should handle null values', () => {
      const yaml = safeDump({ value: null });
      expect(yaml).toContain('value: ~');
    });

    it('should handle booleans', () => {
      const yaml = safeDump({ flag: true, disabled: false });
      expect(yaml).toContain('flag: true');
      expect(yaml).toContain('disabled: false');
    });

    it('should handle special float values', () => {
      const yaml = safeDump({
        inf: Number.POSITIVE_INFINITY,
        negInf: Number.NEGATIVE_INFINITY,
        nan: Number.NaN,
      });
      expect(yaml).toContain('.inf');
      expect(yaml).toContain('-.inf');
      expect(yaml).toContain('.nan');
    });

    it('should sort keys when requested', () => {
      const data = { z: 1, a: 2, m: 3 };
      const yaml = safeDump(data, { sortKeys: true });
      const lines = yaml.split('\n').filter((l) => l.trim());

      // Find indices of each key
      const aIndex = lines.findIndex((l) => l.startsWith('a:'));
      const mIndex = lines.findIndex((l) => l.startsWith('m:'));
      const zIndex = lines.findIndex((l) => l.startsWith('z:'));

      // Verify sorted order
      expect(aIndex).toBeLessThan(mIndex);
      expect(mIndex).toBeLessThan(zIndex);
    });

    it('should not include document separator by default', () => {
      const yaml = safeDump({ test: 'value' });
      expect(yaml).not.toMatch(/^---/);
    });
  });

  describe('safeDumpAll', () => {
    it('should serialize single document', () => {
      const yaml = safeDumpAll([{ name: 'test' }]);
      expect(yaml).toContain('name: test');
    });

    it('should serialize multiple documents with separators', () => {
      const yaml = safeDumpAll([{ a: 1 }, { b: 2 }, { c: 3 }]);
      expect(yaml).toContain('a: 1');
      expect(yaml).toContain('---');
      expect(yaml).toContain('b: 2');
      expect(yaml).toContain('c: 3');

      // Count document separators (one before each document except the first)
      const separators = (yaml.match(/---/g) || []).length;
      expect(separators).toBe(2); // n-1 separators for n documents
    });

    it('should handle empty array', () => {
      const yaml = safeDumpAll([]);
      expect(yaml).toBe('');
    });

    it('should sort keys when requested', () => {
      const docs = [{ z: 1, a: 2 }];
      const yaml = safeDumpAll(docs, { sortKeys: true });
      const lines = yaml.split('\n').filter((l) => l.trim());

      const aIndex = lines.findIndex((l) => l.startsWith('a:'));
      const zIndex = lines.findIndex((l) => l.startsWith('z:'));

      expect(aIndex).toBeLessThan(zIndex);
    });
  });

  describe('Round-trip tests', () => {
    it('should round-trip simple objects', () => {
      const original = { name: 'test', value: 123, flag: true };
      const yaml = safeDump(original);
      const parsed = safeLoad(yaml);
      expect(parsed).toEqual(original);
    });

    it('should round-trip nested structures', () => {
      const original = {
        person: {
          name: 'John',
          age: 30,
          hobbies: ['reading', 'coding'],
        },
      };
      const yaml = safeDump(original);
      const parsed = safeLoad(yaml);
      expect(parsed).toEqual(original);
    });

    it('should round-trip arrays', () => {
      const original = [1, 'two', true, null, { nested: 'object' }];
      const yaml = safeDump(original);
      const parsed = safeLoad(yaml);
      expect(parsed).toEqual(original);
    });

    it('should round-trip multi-document YAML', () => {
      const original = [{ a: 1 }, { b: 2 }, { c: 3 }];
      const yaml = safeDumpAll(original);
      const parsed = safeLoadAll(yaml);
      expect(parsed).toEqual(original);
    });
  });
});
