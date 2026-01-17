/**
 * Unit tests for Schema enum and schema behavior
 */

import { describe, expect, it } from 'vitest';
import { load, loadAll, Schema } from '../index';

describe('Schema Enum', () => {
  describe('schema types', () => {
    it('should have SafeSchema constant', () => {
      expect(Schema.SafeSchema).toBeDefined();
      expect(typeof Schema.SafeSchema).toBe('string');
    });

    it('should have JsonSchema constant', () => {
      expect(Schema.JsonSchema).toBeDefined();
      expect(typeof Schema.JsonSchema).toBe('string');
    });

    it('should have CoreSchema constant', () => {
      expect(Schema.CoreSchema).toBeDefined();
      expect(typeof Schema.CoreSchema).toBe('string');
    });

    it('should have FailsafeSchema constant', () => {
      expect(Schema.FailsafeSchema).toBeDefined();
      expect(typeof Schema.FailsafeSchema).toBe('string');
    });

    it('should have all four schema types', () => {
      const schemas = [
        Schema.SafeSchema,
        Schema.JsonSchema,
        Schema.CoreSchema,
        Schema.FailsafeSchema,
      ];
      expect(schemas).toHaveLength(4);
      expect(new Set(schemas).size).toBe(4); // All unique
    });
  });

  describe('schema behavior with parsing', () => {
    const testYaml = `
name: test
number: 42
bool: true
null_val: null
array:
  - one
  - two
hash:
  key1: value1
  key2: value2
`;

    it('should parse with SafeSchema', () => {
      const result = load(testYaml, { schema: Schema.SafeSchema });
      expect(result).toHaveProperty('name', 'test');
      expect(result).toHaveProperty('number', 42);
      expect(result).toHaveProperty('bool', true);
      expect(result).toHaveProperty('null_val', null);
    });

    it('should parse with JsonSchema', () => {
      const result = load(testYaml, { schema: Schema.JsonSchema });
      expect(result).toHaveProperty('name', 'test');
      expect(result).toHaveProperty('number', 42);
      expect(result).toHaveProperty('bool', true);
    });

    it('should parse with CoreSchema', () => {
      const result = load(testYaml, { schema: Schema.CoreSchema });
      expect(result).toHaveProperty('name', 'test');
      expect(result).toHaveProperty('number', 42);
      expect(result).toHaveProperty('bool', true);
    });

    it('should parse with FailsafeSchema', () => {
      const result = load(testYaml, { schema: Schema.FailsafeSchema });
      expect(result).toHaveProperty('name', 'test');
      expect(result).toHaveProperty('number', 42);
      expect(result).toHaveProperty('bool', true);
    });
  });

  describe('schema with boolean values', () => {
    it('should parse lowercase true/false with SafeSchema', () => {
      const result = load('a: true\nb: false', { schema: Schema.SafeSchema });
      expect(result).toEqual({ a: true, b: false });
    });

    it('should parse boolean values with JsonSchema', () => {
      const result = load('flag: true\ndisabled: false', { schema: Schema.JsonSchema });
      expect(result).toEqual({ flag: true, disabled: false });
    });

    it('should parse boolean values with CoreSchema', () => {
      const result = load('x: true\ny: false', { schema: Schema.CoreSchema });
      expect(result).toEqual({ x: true, y: false });
    });

    it('should parse boolean values with FailsafeSchema', () => {
      const result = load('enabled: true\noff: false', { schema: Schema.FailsafeSchema });
      expect(result).toEqual({ enabled: true, off: false });
    });

    it('should handle uppercase boolean-like strings as strings', () => {
      const result = load('val: TRUE', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: 'TRUE' });
    });

    it('should handle yes/no as strings in YAML 1.2.2', () => {
      const result = load('answer: yes\nnegative: no', { schema: Schema.SafeSchema });
      expect(result).toEqual({ answer: 'yes', negative: 'no' });
    });
  });

  describe('schema with null values', () => {
    it('should parse null with SafeSchema', () => {
      const result = load('val: null', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: null });
    });

    it('should parse ~ as null with SafeSchema', () => {
      const result = load('val: ~', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: null });
    });

    it('should parse empty value as null with SafeSchema', () => {
      const result = load('val:', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: null });
    });

    it('should parse NULL as null', () => {
      const result = load('val: NULL', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: null });
    });

    it('should parse Null as string', () => {
      const result = load('val: Null', { schema: Schema.SafeSchema });
      expect(result).toEqual({ val: 'Null' });
    });

    it('should handle multiple null representations', () => {
      const yaml = `
a: null
b: ~
c:
d: NULL
`;
      const result = load(yaml, { schema: Schema.CoreSchema });
      expect(result).toEqual({ a: null, b: null, c: null, d: null });
    });
  });

  describe('schema with numeric values', () => {
    it('should parse integers with SafeSchema', () => {
      const result = load('num: 123', { schema: Schema.SafeSchema });
      expect(result).toEqual({ num: 123 });
    });

    it('should parse negative integers', () => {
      const result = load('neg: -456', { schema: Schema.SafeSchema });
      expect(result).toEqual({ neg: -456 });
    });

    it('should parse floats with SafeSchema', () => {
      const result = load('float: 3.14', { schema: Schema.SafeSchema });
      expect(result).toEqual({ float: 3.14 });
    });

    it('should parse scientific notation', () => {
      const result = load('sci: 1.23e+4', { schema: Schema.SafeSchema });
      expect(result).toEqual({ sci: 12300 });
    });

    it('should parse hexadecimal with SafeSchema', () => {
      const result = load('hex: 0xFF', { schema: Schema.SafeSchema });
      expect(result).toEqual({ hex: 255 });
    });

    it('should parse octal with SafeSchema', () => {
      const result = load('oct: 0o77', { schema: Schema.SafeSchema });
      expect(result).toEqual({ oct: 63 });
    });

    it('should parse zero', () => {
      const result = load('zero: 0', { schema: Schema.SafeSchema });
      expect(result).toEqual({ zero: 0 });
    });
  });

  describe('schema with special float values', () => {
    it('should parse .inf as Infinity', () => {
      const result = load('val: .inf', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBe(Infinity);
    });

    it('should parse -.inf as -Infinity', () => {
      const result = load('val: -.inf', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBe(-Infinity);
    });

    it('should parse .nan as NaN', () => {
      const result = load('val: .nan', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBeNaN();
    });

    it('should parse .Inf (capitalized) as Infinity', () => {
      const result = load('val: .Inf', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBe(Infinity);
    });

    it('should parse .INF (uppercase) as Infinity', () => {
      const result = load('val: .INF', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBe(Infinity);
    });

    it('should parse .NaN (capitalized) as NaN', () => {
      const result = load('val: .NaN', { schema: Schema.SafeSchema }) as Record<string, number>;
      expect(result.val).toBeNaN();
    });
  });

  describe('schema with arrays', () => {
    it('should parse arrays with SafeSchema', () => {
      const result = load('items:\n  - a\n  - b\n  - c', { schema: Schema.SafeSchema });
      expect(result).toEqual({ items: ['a', 'b', 'c'] });
    });

    it('should parse nested arrays', () => {
      const yaml = `
matrix:
  - [1, 2, 3]
  - [4, 5, 6]
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toEqual({
        matrix: [
          [1, 2, 3],
          [4, 5, 6],
        ],
      });
    });

    it('should parse empty array', () => {
      const result = load('empty: []', { schema: Schema.SafeSchema });
      expect(result).toEqual({ empty: [] });
    });

    it('should parse mixed type array', () => {
      const yaml = `
mixed:
  - string
  - 123
  - true
  - null
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toEqual({ mixed: ['string', 123, true, null] });
    });
  });

  describe('schema with objects', () => {
    it('should parse objects with SafeSchema', () => {
      const yaml = `
person:
  name: John
  age: 30
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toEqual({ person: { name: 'John', age: 30 } });
    });

    it('should parse nested objects', () => {
      const yaml = `
config:
  server:
    host: localhost
    port: 8080
  database:
    name: mydb
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toEqual({
        config: {
          server: { host: 'localhost', port: 8080 },
          database: { name: 'mydb' },
        },
      });
    });

    it('should parse empty object', () => {
      const result = load('empty: {}', { schema: Schema.SafeSchema });
      expect(result).toEqual({ empty: {} });
    });
  });

  describe('schema with multi-document parsing', () => {
    it('should parse multiple documents with SafeSchema', () => {
      const docs = loadAll('---\na: 1\n---\nb: 2', { schema: Schema.SafeSchema });
      expect(docs).toEqual([{ a: 1 }, { b: 2 }]);
    });

    it('should parse multiple documents with JsonSchema', () => {
      const docs = loadAll('---\nx: true\n---\ny: false', { schema: Schema.JsonSchema });
      expect(docs).toEqual([{ x: true }, { y: false }]);
    });

    it('should parse multiple documents with CoreSchema', () => {
      const docs = loadAll('---\nfoo: bar\n---\nbaz: qux', { schema: Schema.CoreSchema });
      expect(docs).toEqual([{ foo: 'bar' }, { baz: 'qux' }]);
    });

    it('should parse multiple documents with FailsafeSchema', () => {
      const docs = loadAll('---\nm: 1\n---\nn: 2\n---\no: 3', {
        schema: Schema.FailsafeSchema,
      });
      expect(docs).toEqual([{ m: 1 }, { n: 2 }, { o: 3 }]);
    });
  });

  describe('schema with complex YAML', () => {
    it('should parse complex nested structures', () => {
      const yaml = `
application:
  name: MyApp
  version: 1.0.0
  settings:
    debug: true
    features:
      - feature1
      - feature2
    limits:
      max_users: 1000
      timeout: 30.5
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toHaveProperty('application.name', 'MyApp');
      expect(result).toHaveProperty('application.version', '1.0.0');
      expect(result).toHaveProperty('application.settings.debug', true);
      expect(result).toHaveProperty('application.settings.features', ['feature1', 'feature2']);
      expect(result).toHaveProperty('application.settings.limits.max_users', 1000);
      expect(result).toHaveProperty('application.settings.limits.timeout', 30.5);
    });

    it('should parse YAML with anchors and aliases', () => {
      const yaml = `
defaults: &defaults
  timeout: 30
  retries: 3

production:
  timeout: 30
  retries: 3
  host: prod.example.com

staging:
  timeout: 30
  retries: 3
  host: staging.example.com
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toHaveProperty('production.timeout', 30);
      expect(result).toHaveProperty('production.retries', 3);
      expect(result).toHaveProperty('staging.timeout', 30);
      expect(result).toHaveProperty('staging.retries', 3);
    });

    it('should parse YAML with quoted strings', () => {
      const yaml = `
single: 'single quoted'
double: "double quoted"
unquoted: plain string
`;
      const result = load(yaml, { schema: Schema.SafeSchema });
      expect(result).toEqual({
        single: 'single quoted',
        double: 'double quoted',
        unquoted: 'plain string',
      });
    });

    it('should parse YAML with multiline strings', () => {
      const yaml = `
literal: |
  This is a
  literal block
folded: >
  This is a
  folded block
`;
      const result = load(yaml, { schema: Schema.SafeSchema }) as Record<string, string>;
      expect(result.literal).toContain('This is a\nliteral block');
      expect(result.folded).toContain('This is a folded block');
    });
  });

  describe('schema API compatibility', () => {
    it('should work with no schema specified (defaults to SafeSchema)', () => {
      const result = load('test: value');
      expect(result).toEqual({ test: 'value' });
    });

    it('should work with undefined schema in options', () => {
      const result = load('test: value', { schema: undefined });
      expect(result).toEqual({ test: 'value' });
    });

    it('should accept schema in options object', () => {
      const result = load('num: 42', { schema: Schema.SafeSchema, filename: 'test.yaml' });
      expect(result).toEqual({ num: 42 });
    });

    it('should be compatible with js-yaml schema usage', () => {
      const schemas = [Schema.SafeSchema, Schema.JsonSchema, Schema.CoreSchema];
      schemas.forEach((schema) => {
        const result = load('key: value', { schema });
        expect(result).toEqual({ key: 'value' });
      });
    });
  });

  describe('schema with error handling', () => {
    it('should return error on invalid YAML regardless of schema', () => {
      const schemas = [
        Schema.SafeSchema,
        Schema.JsonSchema,
        Schema.CoreSchema,
        Schema.FailsafeSchema,
      ];

      schemas.forEach((schema) => {
        const result = load('invalid: [', { schema });
        expect(result).toBeInstanceOf(Error);
      });
    });

    it('should return error on malformed document with schema', () => {
      const result = load('key: {invalid', { schema: Schema.SafeSchema });
      expect(result).toBeInstanceOf(Error);
    });

    it('should validate input size with all schemas', () => {
      const large = 'x: '.repeat(35_000_000); // ~105MB, exceeds 100MB limit
      const schemas = [
        Schema.SafeSchema,
        Schema.JsonSchema,
        Schema.CoreSchema,
        Schema.FailsafeSchema,
      ];

      schemas.forEach((schema) => {
        const result = load(large, { schema });
        expect(result).toBeInstanceOf(Error);
        expect((result as Error).message).toContain('exceeds maximum');
      });
    });
  });
});
