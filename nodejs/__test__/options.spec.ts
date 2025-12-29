/**
 * Unit tests for YAML parser and emitter options
 */

import { describe, it, expect } from 'vitest';
import {
  load,
  loadAll,
  safeDump,
  safeDumpAll,
  Schema,
  type LoadOptions,
  type DumpOptions,
} from '../index';

describe('Parser Options - LoadOptions', () => {
  describe('load() with options', () => {
    it('should parse YAML with default options', () => {
      const result = load('name: test');
      expect(result).toEqual({ name: 'test' });
    });

    it('should parse YAML with SafeSchema option', () => {
      const options: LoadOptions = { schema: Schema.SafeSchema };
      const result = load('name: test', options);
      expect(result).toEqual({ name: 'test' });
    });

    it('should parse YAML with JsonSchema option', () => {
      const options: LoadOptions = { schema: Schema.JsonSchema };
      const result = load('value: 123', options);
      expect(result).toEqual({ value: 123 });
    });

    it('should parse YAML with CoreSchema option', () => {
      const options: LoadOptions = { schema: Schema.CoreSchema };
      const result = load('flag: true', options);
      expect(result).toEqual({ flag: true });
    });

    it('should parse YAML with FailsafeSchema option', () => {
      const options: LoadOptions = { schema: Schema.FailsafeSchema };
      const result = load('test: value', options);
      expect(result).toEqual({ test: 'value' });
    });

    it('should parse YAML with filename option', () => {
      const options: LoadOptions = { filename: 'test.yaml' };
      const result = load('data: 42', options);
      expect(result).toEqual({ data: 42 });
    });

    it('should parse YAML with allow_duplicate_keys option', () => {
      const options: LoadOptions = { allow_duplicate_keys: true };
      const result = load('key: first\nkey: second', options);
      expect(result).toEqual({ key: 'second' });
    });

    it('should parse YAML with all options combined', () => {
      const options: LoadOptions = {
        schema: Schema.SafeSchema,
        filename: 'config.yaml',
        allow_duplicate_keys: false,
      };
      const result = load('host: localhost\nport: 8080', options);
      expect(result).toEqual({ host: 'localhost', port: 8080 });
    });

    it('should handle empty input with options', () => {
      const options: LoadOptions = { schema: Schema.SafeSchema };
      const result = load('', options);
      expect(result).toBe(null);
    });

    it('should return error on invalid YAML with options', () => {
      const options: LoadOptions = { filename: 'bad.yaml' };
      const result = load('invalid: [', options);
      expect(result).toBeInstanceOf(Error);
    });
  });

  describe('loadAll() with options', () => {
    it('should parse multiple documents with default options', () => {
      const docs = loadAll('---\na: 1\n---\nb: 2');
      expect(docs).toHaveLength(2);
      expect(docs[0]).toEqual({ a: 1 });
      expect(docs[1]).toEqual({ b: 2 });
    });

    it('should parse multiple documents with SafeSchema', () => {
      const options: LoadOptions = { schema: Schema.SafeSchema };
      const docs = loadAll('---\nfoo: bar\n---\nbaz: qux', options);
      expect(docs).toHaveLength(2);
      expect(docs[0]).toEqual({ foo: 'bar' });
      expect(docs[1]).toEqual({ baz: 'qux' });
    });

    it('should parse multiple documents with filename', () => {
      const options: LoadOptions = { filename: 'multi.yaml' };
      const docs = loadAll('---\nx: 1\n---\ny: 2\n---\nz: 3', options);
      expect(docs).toHaveLength(3);
      expect(docs[0]).toEqual({ x: 1 });
      expect(docs[1]).toEqual({ y: 2 });
      expect(docs[2]).toEqual({ z: 3 });
    });

    it('should parse single document with options', () => {
      const options: LoadOptions = { schema: Schema.CoreSchema };
      const docs = loadAll('single: document', options);
      expect(docs).toHaveLength(1);
      expect(docs[0]).toEqual({ single: 'document' });
    });

    it('should handle empty input with options', () => {
      const options: LoadOptions = { allow_duplicate_keys: true };
      const docs = loadAll('', options);
      expect(docs).toEqual([]);
    });

    it('should return error on invalid YAML with options', () => {
      const options: LoadOptions = { filename: 'broken.yaml' };
      const result = loadAll('---\nvalid: true\n---\ninvalid: {', options);
      expect(result).toBeInstanceOf(Error);
    });
  });
});

describe('Emitter Options - DumpOptions', () => {
  describe('safeDump() with indent option', () => {
    it('should use default indent of 2 spaces', () => {
      const yaml = safeDump({ parent: { child: 'value' } });
      expect(yaml).toContain('parent:\n  child: value');
    });

    it('should use custom indent of 4 spaces', () => {
      const options: DumpOptions = { indent: 4 };
      const yaml = safeDump({ parent: { child: 'value' } }, options);
      expect(yaml).toContain('parent:');
      expect(yaml).toContain('child: value');
    });

    it('should use custom indent of 1 space', () => {
      const options: DumpOptions = { indent: 1 };
      const yaml = safeDump({ a: { b: 1 } }, options);
      expect(yaml).toContain('a:');
      expect(yaml).toContain('b: 1');
    });

    it('should handle nested objects with custom indent', () => {
      const options: DumpOptions = { indent: 3 };
      const data = {
        level1: {
          level2: {
            level3: 'deep',
          },
        },
      };
      const yaml = safeDump(data, options);
      expect(yaml).toContain('level1:');
      expect(yaml).toContain('level2:');
      expect(yaml).toContain('level3: deep');
    });
  });

  describe('safeDump() with width option', () => {
    it('should use default width of 80', () => {
      const options: DumpOptions = { width: 80 };
      const yaml = safeDump({ key: 'value' }, options);
      expect(yaml).toBeTruthy();
    });

    it('should use custom width of 120', () => {
      const options: DumpOptions = { width: 120 };
      const longString = 'a'.repeat(100);
      const yaml = safeDump({ text: longString }, options);
      expect(yaml).toContain(longString);
    });

    it('should use minimum width of 20', () => {
      const options: DumpOptions = { width: 20 };
      const yaml = safeDump({ short: 'text' }, options);
      expect(yaml).toContain('short: text');
    });
  });

  describe('safeDump() with default_flow_style option', () => {
    it('should use block style by default (null)', () => {
      const yaml = safeDump({ items: ['a', 'b', 'c'] });
      expect(yaml).toContain('items:');
      expect(yaml).toContain('- a');
      expect(yaml).toContain('- b');
      expect(yaml).toContain('- c');
    });

    it('should use flow style when default_flow_style is true', () => {
      const options: DumpOptions = { default_flow_style: true };
      const yaml = safeDump({ items: ['a', 'b', 'c'] }, options);
      expect(yaml).toContain('items:');
      expect(yaml).toBeTruthy();
    });

    it('should use block style when default_flow_style is false', () => {
      const options: DumpOptions = { default_flow_style: false };
      const yaml = safeDump({ items: ['x', 'y'] }, options);
      expect(yaml).toContain('items:');
      expect(yaml).toContain('- x');
      expect(yaml).toContain('- y');
    });

    it('should apply flow style to nested objects', () => {
      const options: DumpOptions = { default_flow_style: true };
      const data = {
        outer: {
          inner: { a: 1, b: 2 },
        },
      };
      const yaml = safeDump(data, options);
      expect(yaml).toContain('outer:');
      expect(yaml).toBeTruthy();
    });
  });

  describe('safeDump() with explicit_start option', () => {
    it('should not include document separator by default', () => {
      const yaml = safeDump({ test: 'value' });
      expect(yaml).not.toMatch(/^---/);
    });

    it('should include document separator when explicit_start is true', () => {
      const options: DumpOptions = { explicit_start: true };
      const yaml = safeDump({ test: 'value' }, options);
      expect(yaml).toContain('test: value');
      expect(yaml).toBeTruthy();
    });

    it('should not include document separator when explicit_start is false', () => {
      const options: DumpOptions = { explicit_start: false };
      const yaml = safeDump({ test: 'value' }, options);
      expect(yaml).not.toMatch(/^---/);
    });
  });

  describe('safeDump() with allow_unicode option', () => {
    it('should handle unicode by default', () => {
      const yaml = safeDump({ text: 'Hello 世界 🌍' });
      expect(yaml).toContain('世界');
      expect(yaml).toContain('🌍');
    });

    it('should handle unicode when allow_unicode is true', () => {
      const options: DumpOptions = { allow_unicode: true };
      const yaml = safeDump({ emoji: '✨🎉' }, options);
      expect(yaml).toContain('✨🎉');
    });

    it('should accept allow_unicode false (API compatibility)', () => {
      const options: DumpOptions = { allow_unicode: false };
      const yaml = safeDump({ text: 'ascii' }, options);
      expect(yaml).toContain('ascii');
    });
  });

  describe('safeDump() with multiple options combined', () => {
    it('should apply sortKeys with indent', () => {
      const options: DumpOptions = { sortKeys: true, indent: 4 };
      const yaml = safeDump({ z: 1, a: 2 }, options);
      const lines = yaml.split('\n').filter((l) => l.trim());
      const aIndex = lines.findIndex((l) => l.startsWith('a:'));
      const zIndex = lines.findIndex((l) => l.startsWith('z:'));
      expect(aIndex).toBeLessThan(zIndex);
    });

    it('should apply all options together', () => {
      const options: DumpOptions = {
        sortKeys: true,
        indent: 3,
        width: 100,
        explicit_start: true,
        allow_unicode: true,
      };
      const data = { z: 'last', a: 'first', m: 'middle' };
      const yaml = safeDump(data, options);
      expect(yaml).toContain('a: first');
      expect(yaml).toContain('m: middle');
      expect(yaml).toContain('z: last');
    });
  });

  describe('safeDumpAll() with options', () => {
    it('should apply sortKeys to all documents', () => {
      const options: DumpOptions = { sortKeys: true };
      const yaml = safeDumpAll([{ z: 1, a: 2 }, { y: 3, b: 4 }], options);
      expect(yaml).toContain('a: 2');
      expect(yaml).toContain('b: 4');
    });

    it('should apply indent to all documents', () => {
      const options: DumpOptions = { indent: 4 };
      const yaml = safeDumpAll([{ x: { y: 1 } }, { a: { b: 2 } }], options);
      expect(yaml).toContain('x:');
      expect(yaml).toContain('y: 1');
      expect(yaml).toContain('a:');
      expect(yaml).toContain('b: 2');
    });

    it('should apply explicit_start to documents', () => {
      const options: DumpOptions = { explicit_start: true };
      const yaml = safeDumpAll([{ a: 1 }, { b: 2 }], options);
      expect(yaml).toContain('a: 1');
      expect(yaml).toContain('b: 2');
    });

    it('should apply flow style to all documents', () => {
      const options: DumpOptions = { default_flow_style: true };
      const yaml = safeDumpAll([{ items: [1, 2] }, { nums: [3, 4] }], options);
      expect(yaml).toContain('items:');
      expect(yaml).toContain('nums:');
    });

    it('should enforce 100MB output size limit', () => {
      const largeArray = Array(500_000)
        .fill(null)
        .map((_, i) => ({ [`key${i}`]: 'x'.repeat(500) }));
      const result = safeDumpAll(largeArray);
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toContain('exceeds maximum');
    });
  });
});
