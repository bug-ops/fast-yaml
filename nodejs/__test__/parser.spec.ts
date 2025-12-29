/**
 * Unit tests for YAML parser functions
 */

import { describe, it, expect } from 'vitest';
import { safeLoad, safeLoadAll, safeDump, safeDumpAll, version } from '../index';

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
      // YAML 1.2 Core Schema: only lowercase true/false are booleans
      expect(safeLoad('value: true')).toEqual({ value: true });
      expect(safeLoad('value: false')).toEqual({ value: false });
      // Capitalized versions are strings in YAML 1.2
      expect(safeLoad('value: TRUE')).toEqual({ value: 'TRUE' });
      expect(safeLoad('value: FALSE')).toEqual({ value: 'FALSE' });
      expect(safeLoad('value: True')).toEqual({ value: 'True' });
      expect(safeLoad('value: False')).toEqual({ value: 'False' });

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

      // Note: saphyr also recognizes Null and NULL as null
      // (extends beyond strict YAML 1.2 Core Schema which only recognizes lowercase null)
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
      const infResult = safeLoad('value: .inf');
      expect((infResult as any).value).toBe(Infinity);

      const negInfResult = safeLoad('value: -.inf');
      expect((negInfResult as any).value).toBe(-Infinity);

      const nanResult = safeLoad('value: .nan');
      expect((nanResult as any).value).toBeNaN();
    });

    it('should handle arrays', () => {
      const result = safeLoad('items:\n  - one\n  - two\n  - three');
      expect(result).toEqual({ items: ['one', 'two', 'three'] });
    });

    it('should handle empty input', () => {
      expect(safeLoad('')).toBe(null);
      expect(safeLoad('   ')).toBe(null);
    });

    it('should return error on invalid YAML', () => {
      // NAPI-RS returns Error objects instead of throwing
      const result1 = safeLoad('invalid: [');
      expect(result1).toBeInstanceOf(Error);
      expect((result1 as Error).message).toContain('YAML parse error');

      const result2 = safeLoad('key: {invalid');
      expect(result2).toBeInstanceOf(Error);
    });

    it('should enforce 100MB size limit', () => {
      // Create a string larger than 100MB
      const large = 'x: '.repeat(50_000_000);
      const result = safeLoad(large);
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toContain('exceeds maximum');
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

    it('should return error on invalid YAML', () => {
      // NAPI-RS returns Error objects instead of throwing
      const result = safeLoadAll('---\nvalid: true\n---\ninvalid: [');
      expect(result).toBeInstanceOf(Error);
    });

    it('should enforce 100MB size limit', () => {
      const large = 'x: '.repeat(50_000_000);
      const result = safeLoadAll(large);
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toContain('exceeds maximum');
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
      const yaml = safeDump({ inf: Infinity, negInf: -Infinity, nan: NaN });
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
