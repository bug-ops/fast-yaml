/**
 * Comprehensive API coverage tests
 * Ensures all exported functions and their combinations are tested
 */

import { describe, it, expect } from 'vitest';
import {
  version,
  safeLoad,
  safeLoadAll,
  load,
  loadAll,
  safeDump,
  safeDumpAll,
  Mark,
  Schema,
  type LoadOptions,
  type DumpOptions,
} from '../index';

describe('API Coverage - All Functions', () => {
  describe('version()', () => {
    it('should return version string', () => {
      const v = version();
      expect(typeof v).toBe('string');
      expect(v).toMatch(/^\d+\.\d+\.\d+/);
    });

    it('should return consistent version', () => {
      const v1 = version();
      const v2 = version();
      expect(v1).toBe(v2);
    });
  });

  describe('safeLoad() - all variations', () => {
    it('should parse primitive values', () => {
      expect(safeLoad('string_value')).toBe('string_value');
      expect(safeLoad('123')).toBe(123);
      expect(safeLoad('true')).toBe(true);
      expect(safeLoad('false')).toBe(false);
      expect(safeLoad('null')).toBe(null);
      expect(safeLoad('~')).toBe(null);
    });

    it('should parse objects', () => {
      expect(safeLoad('key: value')).toEqual({ key: 'value' });
      expect(safeLoad('a: 1\nb: 2')).toEqual({ a: 1, b: 2 });
    });

    it('should parse arrays', () => {
      expect(safeLoad('[1, 2, 3]')).toEqual([1, 2, 3]);
      expect(safeLoad('- a\n- b\n- c')).toEqual(['a', 'b', 'c']);
    });

    it('should handle empty strings', () => {
      expect(safeLoad('')).toBe(null);
      expect(safeLoad('  ')).toBe(null);
      expect(safeLoad('\n\n')).toBe(null);
    });

    it('should handle whitespace', () => {
      expect(safeLoad('  key: value  ')).toEqual({ key: 'value' });
      expect(safeLoad('\n\nkey: value\n\n')).toEqual({ key: 'value' });
    });

    it('should return errors for invalid YAML', () => {
      expect(safeLoad('{')).toBeInstanceOf(Error);
      expect(safeLoad('[')).toBeInstanceOf(Error);
      expect(safeLoad('key: {')).toBeInstanceOf(Error);
      expect(safeLoad('- [')).toBeInstanceOf(Error);
    });
  });

  describe('load() - with options', () => {
    it('should work without options', () => {
      expect(load('key: value')).toEqual({ key: 'value' });
    });

    it('should work with null options', () => {
      expect(load('key: value', null)).toEqual({ key: 'value' });
    });

    it('should work with undefined options', () => {
      expect(load('key: value', undefined)).toEqual({ key: 'value' });
    });

    it('should work with empty options object', () => {
      expect(load('key: value', {})).toEqual({ key: 'value' });
    });

    it('should work with SafeSchema', () => {
      expect(load('key: value', { schema: Schema.SafeSchema })).toEqual({ key: 'value' });
    });

    it('should work with JsonSchema', () => {
      expect(load('key: value', { schema: Schema.JsonSchema })).toEqual({ key: 'value' });
    });

    it('should work with CoreSchema', () => {
      expect(load('key: value', { schema: Schema.CoreSchema })).toEqual({ key: 'value' });
    });

    it('should work with FailsafeSchema', () => {
      expect(load('key: value', { schema: Schema.FailsafeSchema })).toEqual({ key: 'value' });
    });

    it('should work with filename option', () => {
      expect(load('key: value', { filename: 'test.yaml' })).toEqual({ key: 'value' });
      expect(load('key: value', { filename: '/path/to/file.yaml' })).toEqual({ key: 'value' });
      expect(load('key: value', { filename: '<stdin>' })).toEqual({ key: 'value' });
    });

    it('should work with allow_duplicate_keys option', () => {
      const yaml = 'key: first\nkey: second';
      expect(load(yaml, { allow_duplicate_keys: true })).toEqual({ key: 'second' });
      expect(load(yaml, { allow_duplicate_keys: false })).toEqual({ key: 'second' });
    });

    it('should work with combined options', () => {
      const options: LoadOptions = {
        schema: Schema.SafeSchema,
        filename: 'config.yaml',
        allow_duplicate_keys: true,
      };
      expect(load('key: value', options)).toEqual({ key: 'value' });
    });

    it('should return errors with options', () => {
      const options: LoadOptions = { filename: 'bad.yaml' };
      expect(load('invalid: [', options)).toBeInstanceOf(Error);
    });
  });

  describe('safeLoadAll() - all variations', () => {
    it('should parse empty string', () => {
      expect(safeLoadAll('')).toEqual([]);
      expect(safeLoadAll('  ')).toEqual([]);
    });

    it('should parse single document', () => {
      expect(safeLoadAll('key: value')).toEqual([{ key: 'value' }]);
    });

    it('should parse multiple documents', () => {
      expect(safeLoadAll('---\na: 1\n---\nb: 2')).toEqual([{ a: 1 }, { b: 2 }]);
      expect(safeLoadAll('---\nx: 1\n---\ny: 2\n---\nz: 3')).toEqual([
        { x: 1 },
        { y: 2 },
        { z: 3 },
      ]);
    });

    it('should parse documents with separators', () => {
      expect(safeLoadAll('---\nfoo: bar')).toEqual([{ foo: 'bar' }]);
      expect(safeLoadAll('---\nfoo: bar\n---\nbaz: qux')).toEqual([
        { foo: 'bar' },
        { baz: 'qux' },
      ]);
    });

    it('should parse mixed types', () => {
      expect(safeLoadAll('---\nstring\n---\n123\n---\ntrue')).toEqual(['string', 123, true]);
    });

    it('should return errors for invalid YAML', () => {
      expect(safeLoadAll('---\nvalid: true\n---\ninvalid: [')).toBeInstanceOf(Error);
    });
  });

  describe('loadAll() - with options', () => {
    it('should work without options', () => {
      expect(loadAll('---\na: 1\n---\nb: 2')).toEqual([{ a: 1 }, { b: 2 }]);
    });

    it('should work with null options', () => {
      expect(loadAll('---\na: 1', null)).toEqual([{ a: 1 }]);
    });

    it('should work with empty options', () => {
      expect(loadAll('---\na: 1', {})).toEqual([{ a: 1 }]);
    });

    it('should work with all schema types', () => {
      const yaml = '---\nx: 1\n---\ny: 2';
      expect(loadAll(yaml, { schema: Schema.SafeSchema })).toEqual([{ x: 1 }, { y: 2 }]);
      expect(loadAll(yaml, { schema: Schema.JsonSchema })).toEqual([{ x: 1 }, { y: 2 }]);
      expect(loadAll(yaml, { schema: Schema.CoreSchema })).toEqual([{ x: 1 }, { y: 2 }]);
      expect(loadAll(yaml, { schema: Schema.FailsafeSchema })).toEqual([{ x: 1 }, { y: 2 }]);
    });

    it('should work with filename', () => {
      expect(loadAll('---\na: 1', { filename: 'multi.yaml' })).toEqual([{ a: 1 }]);
    });

    it('should work with combined options', () => {
      const options: LoadOptions = {
        schema: Schema.CoreSchema,
        filename: 'docs.yaml',
        allow_duplicate_keys: true,
      };
      expect(loadAll('---\nx: 1\n---\ny: 2', options)).toEqual([{ x: 1 }, { y: 2 }]);
    });
  });

  describe('safeDump() - all variations', () => {
    it('should dump primitive values', () => {
      expect(safeDump(null).trim()).toBe('~');
      expect(safeDump(true).trim()).toBe('true');
      expect(safeDump(false).trim()).toBe('false');
      expect(safeDump(123).trim()).toBe('123');
      expect(safeDump(3.14)).toContain('3.14');
      expect(safeDump('text').trim()).toBe('text');
    });

    it('should dump objects', () => {
      const yaml = safeDump({ a: 1, b: 2 });
      expect(yaml).toContain('a: 1');
      expect(yaml).toContain('b: 2');
    });

    it('should dump arrays', () => {
      const yaml = safeDump([1, 2, 3]);
      expect(yaml).toContain('- 1');
      expect(yaml).toContain('- 2');
      expect(yaml).toContain('- 3');
    });

    it('should dump empty collections', () => {
      expect(safeDump({}).trim()).toBe('{}');
      expect(safeDump([]).trim()).toBe('[]');
    });

    it('should dump nested structures', () => {
      const data = {
        outer: {
          inner: {
            deep: 'value',
          },
        },
      };
      const yaml = safeDump(data);
      expect(yaml).toContain('outer:');
      expect(yaml).toContain('inner:');
      expect(yaml).toContain('deep: value');
    });

    it('should work without options', () => {
      expect(safeDump({ key: 'value' })).toBeTruthy();
    });

    it('should work with null options', () => {
      expect(safeDump({ key: 'value' }, null)).toBeTruthy();
    });

    it('should work with empty options', () => {
      expect(safeDump({ key: 'value' }, {})).toBeTruthy();
    });
  });

  describe('safeDump() - with DumpOptions', () => {
    it('should work with sortKeys option', () => {
      const yaml = safeDump({ z: 1, a: 2, m: 3 }, { sortKeys: true });
      const lines = yaml.split('\n').filter((l) => l.trim());
      const aIndex = lines.findIndex((l) => l.startsWith('a:'));
      const mIndex = lines.findIndex((l) => l.startsWith('m:'));
      const zIndex = lines.findIndex((l) => l.startsWith('z:'));
      expect(aIndex).toBeLessThan(mIndex);
      expect(mIndex).toBeLessThan(zIndex);
    });

    it('should work with sortKeys false', () => {
      const yaml = safeDump({ z: 1, a: 2 }, { sortKeys: false });
      expect(yaml).toBeTruthy();
    });

    it('should work with indent option', () => {
      const options: DumpOptions[] = [
        { indent: 1 },
        { indent: 2 },
        { indent: 3 },
        { indent: 4 },
        { indent: 6 },
        { indent: 8 },
      ];
      options.forEach((opt) => {
        const yaml = safeDump({ a: { b: 1 } }, opt);
        expect(yaml).toBeTruthy();
      });
    });

    it('should work with width option', () => {
      const options: DumpOptions[] = [
        { width: 20 },
        { width: 40 },
        { width: 80 },
        { width: 120 },
        { width: 200 },
      ];
      options.forEach((opt) => {
        const yaml = safeDump({ key: 'value' }, opt);
        expect(yaml).toBeTruthy();
      });
    });

    it('should work with default_flow_style option', () => {
      const data = { items: [1, 2, 3] };
      expect(safeDump(data, { default_flow_style: true })).toBeTruthy();
      expect(safeDump(data, { default_flow_style: false })).toBeTruthy();
    });

    it('should work with explicit_start option', () => {
      expect(safeDump({ key: 'value' }, { explicit_start: true })).toBeTruthy();
      expect(safeDump({ key: 'value' }, { explicit_start: false })).toBeTruthy();
    });

    it('should work with allow_unicode option', () => {
      const data = { text: 'Hello 世界' };
      expect(safeDump(data, { allow_unicode: true })).toContain('世界');
      expect(safeDump(data, { allow_unicode: false })).toBeTruthy();
    });

    it('should work with all options combined', () => {
      const options: DumpOptions = {
        sortKeys: true,
        indent: 4,
        width: 100,
        default_flow_style: false,
        explicit_start: true,
        allow_unicode: true,
      };
      const yaml = safeDump({ z: 1, a: 2 }, options);
      expect(yaml).toBeTruthy();
    });
  });

  describe('safeDumpAll() - all variations', () => {
    it('should dump empty array', () => {
      expect(safeDumpAll([])).toBe('');
    });

    it('should dump single document', () => {
      const yaml = safeDumpAll([{ a: 1 }]);
      expect(yaml).toContain('a: 1');
    });

    it('should dump multiple documents', () => {
      const yaml = safeDumpAll([{ a: 1 }, { b: 2 }, { c: 3 }]);
      expect(yaml).toContain('a: 1');
      expect(yaml).toContain('b: 2');
      expect(yaml).toContain('c: 3');
      expect(yaml).toContain('---');
    });

    it('should work with primitives', () => {
      const yaml = safeDumpAll([1, 'text', true, null]);
      expect(yaml).toContain('1');
      expect(yaml).toContain('text');
      expect(yaml).toContain('true');
    });

    it('should work without options', () => {
      expect(safeDumpAll([{ a: 1 }])).toBeTruthy();
    });

    it('should work with null options', () => {
      expect(safeDumpAll([{ a: 1 }], null)).toBeTruthy();
    });

    it('should work with empty options', () => {
      expect(safeDumpAll([{ a: 1 }], {})).toBeTruthy();
    });

    it('should work with all DumpOptions', () => {
      const options: DumpOptions = {
        sortKeys: true,
        indent: 2,
        width: 80,
        explicit_start: true,
      };
      const yaml = safeDumpAll([{ z: 1, a: 2 }, { y: 3, b: 4 }], options);
      expect(yaml).toBeTruthy();
    });
  });

  describe('Mark class - all variations', () => {
    it('should create Mark with all parameters', () => {
      const mark = new Mark('file.yaml', 10, 5);
      expect(mark.name).toBe('file.yaml');
      expect(mark.line).toBe(10);
      expect(mark.column).toBe(5);
    });

    it('should create Mark with zero values', () => {
      const mark = new Mark('file.yaml', 0, 0);
      expect(mark.line).toBe(0);
      expect(mark.column).toBe(0);
    });

    it('should create Mark with large numbers', () => {
      const mark = new Mark('file.yaml', 999999, 888888);
      expect(mark.line).toBe(999999);
      expect(mark.column).toBe(888888);
    });

    it('should create Mark with various filenames', () => {
      const marks = [
        new Mark('<input>', 1, 1),
        new Mark('', 1, 1),
        new Mark('/path/to/file.yaml', 1, 1),
        new Mark('C:\\Windows\\path.yaml', 1, 1),
        new Mark('file.yaml', 1, 1),
      ];
      expect(marks).toHaveLength(5);
    });

    it('should have toString method', () => {
      const mark = new Mark('test.yaml', 42, 15);
      expect(mark.toString()).toBe('test.yaml:42:15');
    });

    it('should work with unicode filenames', () => {
      const mark = new Mark('配置.yaml', 1, 1);
      expect(mark.toString()).toBe('配置.yaml:1:1');
    });
  });

  describe('Schema enum - all values', () => {
    it('should have all schema constants', () => {
      expect(Schema.SafeSchema).toBeDefined();
      expect(Schema.JsonSchema).toBeDefined();
      expect(Schema.CoreSchema).toBeDefined();
      expect(Schema.FailsafeSchema).toBeDefined();
    });

    it('should work with load function', () => {
      const yaml = 'key: value';
      expect(load(yaml, { schema: Schema.SafeSchema })).toEqual({ key: 'value' });
      expect(load(yaml, { schema: Schema.JsonSchema })).toEqual({ key: 'value' });
      expect(load(yaml, { schema: Schema.CoreSchema })).toEqual({ key: 'value' });
      expect(load(yaml, { schema: Schema.FailsafeSchema })).toEqual({ key: 'value' });
    });

    it('should work with loadAll function', () => {
      const yaml = '---\na: 1\n---\nb: 2';
      expect(loadAll(yaml, { schema: Schema.SafeSchema })).toHaveLength(2);
      expect(loadAll(yaml, { schema: Schema.JsonSchema })).toHaveLength(2);
      expect(loadAll(yaml, { schema: Schema.CoreSchema })).toHaveLength(2);
      expect(loadAll(yaml, { schema: Schema.FailsafeSchema })).toHaveLength(2);
    });
  });
});

describe('API Coverage - Data Type Combinations', () => {
  describe('all YAML data types', () => {
    it('should handle scalars', () => {
      expect(safeLoad('null')).toBe(null);
      expect(safeLoad('true')).toBe(true);
      expect(safeLoad('false')).toBe(false);
      expect(safeLoad('123')).toBe(123);
      expect(safeLoad('3.14')).toBe(3.14);
      expect(safeLoad('text')).toBe('text');
      expect(safeLoad('"quoted"')).toBe('quoted');
      expect(safeLoad("'single'")).toBe('single');
    });

    it('should handle special numeric values', () => {
      expect((safeLoad('.inf') as any)).toBe(Infinity);
      expect((safeLoad('-.inf') as any)).toBe(-Infinity);
      expect((safeLoad('.nan') as any)).toBeNaN();
      expect(safeLoad('0x10')).toBe(16);
      expect(safeLoad('0o10')).toBe(8);
    });

    it('should handle sequences', () => {
      expect(safeLoad('[]')).toEqual([]);
      expect(safeLoad('[1, 2, 3]')).toEqual([1, 2, 3]);
      expect(safeLoad('- a\n- b')).toEqual(['a', 'b']);
    });

    it('should handle mappings', () => {
      expect(safeLoad('{}')).toEqual({});
      expect(safeLoad('{a: 1, b: 2}')).toEqual({ a: 1, b: 2 });
      expect(safeLoad('a: 1\nb: 2')).toEqual({ a: 1, b: 2 });
    });

    it('should handle nested structures', () => {
      const yaml = `
        users:
          - name: Alice
            age: 30
            active: true
          - name: Bob
            age: 25
            active: false
      `;
      const result = safeLoad(yaml);
      expect(result).toHaveProperty('users');
      expect((result as any).users).toHaveLength(2);
    });
  });

  describe('round-trip all data types', () => {
    const testCases = [
      { name: 'null', value: null },
      { name: 'boolean true', value: true },
      { name: 'boolean false', value: false },
      { name: 'integer', value: 42 },
      { name: 'negative integer', value: -100 },
      { name: 'float', value: 3.14159 },
      { name: 'string', value: 'hello world' },
      { name: 'empty string', value: '' },
      { name: 'empty array', value: [] },
      { name: 'empty object', value: {} },
      { name: 'array of numbers', value: [1, 2, 3, 4, 5] },
      { name: 'array of strings', value: ['a', 'b', 'c'] },
      { name: 'array of mixed', value: [1, 'two', true, null] },
      { name: 'simple object', value: { a: 1, b: 2 } },
      { name: 'nested object', value: { outer: { inner: 'value' } } },
      { name: 'nested array', value: [[1, 2], [3, 4]] },
      { name: 'complex structure', value: { users: [{ name: 'Alice' }, { name: 'Bob' }] } },
    ];

    testCases.forEach(({ name, value }) => {
      it(`should round-trip ${name}`, () => {
        const yaml = safeDump(value);
        const parsed = safeLoad(yaml);
        expect(parsed).toEqual(value);
      });
    });
  });
});

describe('API Coverage - Error Handling', () => {
  describe('all error types', () => {
    it('should handle syntax errors', () => {
      const invalid = ['[', '{', 'key: [', 'key: {', '- [', 'key:\n\tvalue'];
      invalid.forEach((yaml) => {
        expect(safeLoad(yaml)).toBeInstanceOf(Error);
        expect(load(yaml)).toBeInstanceOf(Error);
      });
    });

    it('should handle size limit errors', () => {
      const large = 'x: '.repeat(35_000_000); // ~105MB, exceeds 100MB limit
      expect(safeLoad(large)).toBeInstanceOf(Error);
      expect(load(large)).toBeInstanceOf(Error);
      expect(safeLoadAll(large)).toBeInstanceOf(Error);
      expect(loadAll(large)).toBeInstanceOf(Error);
    });

    it('should handle invalid anchors', () => {
      expect(safeLoad('ref: *unknown')).toBeInstanceOf(Error);
    });

    it('should provide error messages', () => {
      const result = safeLoad('invalid: [');
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toBeTruthy();
    });
  });
});
