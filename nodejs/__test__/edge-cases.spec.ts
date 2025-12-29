/**
 * Unit tests for edge cases and error handling
 */

import { describe, it, expect } from 'vitest';
import { safeLoad, safeLoadAll, safeDump, safeDumpAll } from '../index';

describe('Edge Cases - Parser', () => {
  describe('empty and whitespace input', () => {
    it('should handle completely empty string', () => {
      expect(safeLoad('')).toBe(null);
    });

    it('should handle string with only spaces', () => {
      expect(safeLoad('   ')).toBe(null);
    });

    it('should handle string with only tabs', () => {
      expect(safeLoad('\t\t\t')).toBe(null);
    });

    it('should handle string with only newlines', () => {
      expect(safeLoad('\n\n\n')).toBe(null);
    });

    it('should handle string with mixed whitespace', () => {
      expect(safeLoad(' \t \n \r\n ')).toBe(null);
    });

    it('should handle YAML comment only', () => {
      expect(safeLoad('# Just a comment')).toBe(null);
    });

    it('should handle multiple comments', () => {
      expect(safeLoad('# Comment 1\n# Comment 2\n# Comment 3')).toBe(null);
    });
  });

  describe('special characters and encoding', () => {
    it('should handle unicode characters', () => {
      const result = safeLoad('text: Hello 世界 🌍');
      expect(result).toEqual({ text: 'Hello 世界 🌍' });
    });

    it('should handle emoji in values', () => {
      const result = safeLoad('emoji: 🎉✨🚀');
      expect(result).toEqual({ emoji: '🎉✨🚀' });
    });

    it('should handle emoji in keys', () => {
      const result = safeLoad('🔑: value');
      expect(result).toHaveProperty('🔑', 'value');
    });

    it('should handle right-to-left text', () => {
      const result = safeLoad('arabic: مرحبا');
      expect(result).toEqual({ arabic: 'مرحبا' });
    });

    it('should handle mixed scripts', () => {
      const result = safeLoad('mixed: Hello Мир 世界');
      expect(result).toEqual({ mixed: 'Hello Мир 世界' });
    });

    it('should handle zero-width characters', () => {
      const result = safeLoad('text: hello\u200Bworld');
      expect(result).toEqual({ text: 'hello\u200Bworld' });
    });

    it('should handle combining characters', () => {
      const result = safeLoad('accented: café');
      expect(result).toEqual({ accented: 'café' });
    });

    it('should handle control characters in strings', () => {
      const result = safeLoad('control: "line1\\nline2\\ttab"');
      expect((result as any).control).toContain('line1');
      expect((result as any).control).toContain('line2');
    });
  });

  describe('extreme nesting', () => {
    it('should handle deeply nested objects', () => {
      const yaml = `
a:
  b:
    c:
      d:
        e:
          f:
            g:
              h:
                i:
                  j: deep
`;
      const result = safeLoad(yaml);
      expect(result).toHaveProperty('a.b.c.d.e.f.g.h.i.j', 'deep');
    });

    it('should handle deeply nested arrays', () => {
      const yaml = `
- - - - - - - - - - deep
`;
      const result = safeLoad(yaml);
      expect(Array.isArray(result)).toBe(true);
    });

    it('should handle mixed nesting', () => {
      const yaml = `
level1:
  - level2a:
      - level3a:
          key: value
`;
      const result = safeLoad(yaml);
      expect(result).toHaveProperty('level1');
    });
  });

  describe('boundary values', () => {
    it('should handle very small integer', () => {
      const result = safeLoad('num: -2147483648');
      expect(result).toEqual({ num: -2147483648 });
    });

    it('should handle very large integer', () => {
      const result = safeLoad('num: 2147483647');
      expect(result).toEqual({ num: 2147483647 });
    });

    it('should handle very small float', () => {
      const result = safeLoad('num: -1.7976931348623157e+308');
      expect((result as any).num).toBeCloseTo(-1.7976931348623157e308);
    });

    it('should handle very large float', () => {
      const result = safeLoad('num: 1.7976931348623157e+308');
      expect((result as any).num).toBeCloseTo(1.7976931348623157e308);
    });

    it('should handle very small positive number', () => {
      const result = safeLoad('num: 5e-324');
      expect(result).toHaveProperty('num');
    });

    it('should handle zero in various forms', () => {
      const result = safeLoad('a: 0\nb: 0.0\nc: -0\nd: +0');
      expect((result as any).a).toBe(0);
      expect((result as any).b).toBe(0);
      expect((result as any).c).toBe(0);
      expect((result as any).d).toBe(0);
    });
  });

  describe('unusual but valid YAML', () => {
    it('should handle document with only document separator', () => {
      const result = safeLoad('---');
      expect(result).toBe(null);
    });

    it('should handle document with multiple separators', () => {
      // Three separators = three empty documents = [null, null, null]
      const docs = safeLoadAll('---\n---\n---');
      expect(docs).toEqual([null, null, null]);
    });

    it('should handle key with colon in quoted string', () => {
      const result = safeLoad('"key:with:colons": value');
      expect(result).toHaveProperty('key:with:colons', 'value');
    });

    it('should handle value starting with special characters', () => {
      const result = safeLoad('key: "@value"');
      expect(result).toEqual({ key: '@value' });
    });

    it('should handle numeric-looking strings', () => {
      const result = safeLoad('zip: "12345"');
      expect(result).toEqual({ zip: '12345' });
    });

    it('should handle boolean-looking strings', () => {
      const result = safeLoad('status: "true"');
      expect(result).toEqual({ status: 'true' });
    });

    it('should handle duplicate keys (last wins)', () => {
      const result = safeLoad('key: first\nkey: second\nkey: third');
      expect(result).toEqual({ key: 'third' });
    });

    it('should handle extremely long key', () => {
      const longKey = 'k'.repeat(1000);
      const result = safeLoad(`${longKey}: value`);
      expect(result).toHaveProperty(longKey, 'value');
    });

    it('should handle extremely long value', () => {
      const longValue = 'v'.repeat(10000);
      const result = safeLoad(`key: ${longValue}`);
      expect((result as any).key).toBe(longValue);
    });

    it('should handle array with single item', () => {
      const result = safeLoad('items:\n  - single');
      expect(result).toEqual({ items: ['single'] });
    });

    it('should handle object with single property', () => {
      const result = safeLoad('obj:\n  only: value');
      expect(result).toEqual({ obj: { only: 'value' } });
    });
  });

  describe('malformed YAML', () => {
    it('should return error for unclosed bracket', () => {
      const result = safeLoad('array: [1, 2, 3');
      expect(result).toBeInstanceOf(Error);
    });

    it('should return error for unclosed brace', () => {
      const result = safeLoad('object: {key: value');
      expect(result).toBeInstanceOf(Error);
    });

    it('should parse mixed indentation as valid YAML', () => {
      // This is actually valid YAML - parses as an object
      const result = safeLoad('key:\n value\n  invalid');
      expect(result).not.toBeInstanceOf(Error);
    });

    it('should return error for tab in indentation', () => {
      const result = safeLoad('key:\n\tvalue');
      expect(result).toBeInstanceOf(Error);
    });

    it('should parse plain string without colon', () => {
      // 'key value' is valid YAML - it's a plain string
      const result = safeLoad('key value');
      expect(result).toBe('key value');
    });

    it('should return error for invalid array syntax', () => {
      const result = safeLoad('items:\n  - item1\n - item2');
      expect(result).toBeInstanceOf(Error);
    });

    it('should return error for invalid alias', () => {
      const result = safeLoad('ref: *unknown_anchor');
      expect(result).toBeInstanceOf(Error);
    });

    it('should allow duplicate anchors (later overwrites)', () => {
      // Duplicate anchors are allowed in YAML - the second definition wins
      const result = safeLoad('a: &anchor value\nb: &anchor other');
      expect(result).toEqual({ a: 'value', b: 'other' });
    });
  });

  describe('security and DoS prevention', () => {
    // NOTE: 100MB size limit tests are covered in parser.spec.ts and schema.spec.ts
    // They are skipped here to avoid memory pressure during test runs

    it.skip('should enforce 100MB size limit', () => {
      const large = 'key: value\n'.repeat(10_000_000);
      const result = safeLoad(large);
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toContain('exceeds maximum');
      expect((result as Error).message).toContain('100MB');
    });

    it.skip('should enforce size limit for safeLoadAll', () => {
      // 8M * 14 bytes = 112MB, exceeds 100MB limit
      const large = '---\nkey: value\n'.repeat(8_000_000);
      const result = safeLoadAll(large);
      expect(result).toBeInstanceOf(Error);
      expect((result as Error).message).toContain('exceeds maximum');
    });

    it('should handle very large number of documents', () => {
      const manyDocs = '---\nkey: value\n'.repeat(10000);
      const result = safeLoadAll(manyDocs);
      if (!Array.isArray(result)) {
        expect(result).toBeInstanceOf(Error);
      }
    });

    it('should handle very wide objects (many keys)', () => {
      const keys = Array(10000)
        .fill(null)
        .map((_, i) => `key${i}: value${i}`)
        .join('\n');
      const result = safeLoad(keys);
      if (typeof result === 'object' && result !== null) {
        expect(Object.keys(result).length).toBe(10000);
      }
    });

    it('should handle very long arrays', () => {
      const items = Array(10000)
        .fill(null)
        .map((_, i) => `  - item${i}`)
        .join('\n');
      const yaml = `items:\n${items}`;
      const result = safeLoad(yaml);
      if (typeof result === 'object' && result !== null) {
        expect((result as any).items.length).toBe(10000);
      }
    });
  });
});

describe('Edge Cases - Emitter', () => {
  describe('empty and null values', () => {
    it('should serialize null', () => {
      const yaml = safeDump(null);
      expect(yaml.trim()).toBe('~');
    });

    it('should serialize empty object', () => {
      const yaml = safeDump({});
      expect(yaml.trim()).toBe('{}');
    });

    it('should serialize empty array', () => {
      const yaml = safeDump([]);
      expect(yaml.trim()).toBe('[]');
    });

    it('should serialize object with null values', () => {
      const yaml = safeDump({ a: null, b: null });
      expect(yaml).toContain('a: ~');
      expect(yaml).toContain('b: ~');
    });

    it('should serialize array with null values', () => {
      const yaml = safeDump([null, null, null]);
      expect(yaml).toContain('- ~');
    });
  });

  describe('special characters in output', () => {
    it('should serialize unicode characters', () => {
      const yaml = safeDump({ text: '世界 🌍' });
      expect(yaml).toContain('世界');
      expect(yaml).toContain('🌍');
    });

    it('should serialize emoji', () => {
      const yaml = safeDump({ emoji: '🎉🚀✨' });
      expect(yaml).toContain('🎉');
      expect(yaml).toContain('🚀');
      expect(yaml).toContain('✨');
    });

    it('should serialize newlines in strings', () => {
      const yaml = safeDump({ multiline: 'line1\nline2\nline3' });
      expect(yaml).toBeTruthy();
    });

    it('should serialize special YAML characters', () => {
      const yaml = safeDump({ special: 'colon: dash- bracket[' });
      expect(yaml).toBeTruthy();
    });
  });

  describe('extreme values', () => {
    it('should serialize very large numbers', () => {
      const yaml = safeDump({ big: Number.MAX_SAFE_INTEGER });
      expect(yaml).toContain(Number.MAX_SAFE_INTEGER.toString());
    });

    it('should serialize very small numbers', () => {
      const yaml = safeDump({ small: Number.MIN_SAFE_INTEGER });
      expect(yaml).toContain(Number.MIN_SAFE_INTEGER.toString());
    });

    it('should serialize Infinity', () => {
      const yaml = safeDump({ inf: Infinity });
      expect(yaml).toContain('.inf');
    });

    it('should serialize -Infinity', () => {
      const yaml = safeDump({ negInf: -Infinity });
      expect(yaml).toContain('-.inf');
    });

    it('should serialize NaN', () => {
      const yaml = safeDump({ nan: NaN });
      expect(yaml).toContain('.nan');
    });

    it('should serialize zero in different forms', () => {
      const yaml = safeDump({ a: 0, b: -0, c: +0 });
      expect(yaml).toContain('0');
    });
  });

  describe('deeply nested structures', () => {
    it('should serialize deeply nested objects', () => {
      const deep = { a: { b: { c: { d: { e: { f: 'deep' } } } } } };
      const yaml = safeDump(deep);
      expect(yaml).toContain('f: deep');
    });

    it('should serialize deeply nested arrays', () => {
      const deep = [[[[[['deep']]]]]];
      const yaml = safeDump(deep);
      expect(yaml).toContain('deep');
    });

    it('should serialize mixed deep nesting', () => {
      const mixed = { a: [{ b: [{ c: 'value' }] }] };
      const yaml = safeDump(mixed);
      expect(yaml).toContain('value');
    });
  });

  describe('large data structures', () => {
    it('should serialize object with many keys', () => {
      const manyKeys = Object.fromEntries(
        Array(1000)
          .fill(null)
          .map((_, i) => [`key${i}`, `value${i}`]),
      );
      const yaml = safeDump(manyKeys);
      expect(yaml).toContain('key0');
      expect(yaml).toContain('key999');
    });

    it('should serialize large array', () => {
      const largeArray = Array(1000)
        .fill(null)
        .map((_, i) => i);
      const yaml = safeDump(largeArray);
      expect(yaml).toContain('0');
      expect(yaml).toContain('999');
    });

    it('should handle very long string values', () => {
      const longString = 'x'.repeat(10000);
      const yaml = safeDump({ long: longString });
      expect(yaml).toBeTruthy();
    });

    it('should handle very long keys', () => {
      const longKey = 'k'.repeat(1000);
      const obj = { [longKey]: 'value' };
      const yaml = safeDump(obj);
      expect(yaml).toContain('value');
    });
  });

  describe('unusual but valid JavaScript values', () => {
    it('should serialize boolean true', () => {
      const yaml = safeDump(true);
      expect(yaml.trim()).toBe('true');
    });

    it('should serialize boolean false', () => {
      const yaml = safeDump(false);
      expect(yaml.trim()).toBe('false');
    });

    it('should serialize string', () => {
      const yaml = safeDump('plain string');
      expect(yaml.trim()).toBe('plain string');
    });

    it('should serialize number', () => {
      const yaml = safeDump(42);
      expect(yaml.trim()).toBe('42');
    });

    it('should serialize float', () => {
      const yaml = safeDump(3.14159);
      expect(yaml).toContain('3.14159');
    });

    it('should handle empty string', () => {
      const yaml = safeDump({ empty: '' });
      expect(yaml).toContain("''");
    });

    it('should handle string with only spaces', () => {
      const yaml = safeDump({ spaces: '   ' });
      expect(yaml).toBeTruthy();
    });
  });

  describe('safeDumpAll edge cases', () => {
    it('should serialize empty array of documents', () => {
      const yaml = safeDumpAll([]);
      expect(yaml).toBe('');
    });

    it('should serialize single document', () => {
      const yaml = safeDumpAll([{ key: 'value' }]);
      expect(yaml).toContain('key: value');
    });

    it('should serialize many small documents', () => {
      const docs = Array(1000)
        .fill(null)
        .map((_, i) => ({ [`key${i}`]: i }));
      const yaml = safeDumpAll(docs);
      expect(yaml).toContain('---');
      expect(yaml.split('---').length).toBeGreaterThan(1);
    });

    it('should serialize documents with different types', () => {
      const yaml = safeDumpAll([{ obj: 'value' }, ['array', 'items'], 'string', 42, true, null]);
      expect(yaml).toContain('obj: value');
      expect(yaml).toContain('array');
      expect(yaml).toContain('string');
      expect(yaml).toContain('42');
      expect(yaml).toContain('true');
    });

    it('should serialize documents with unicode', () => {
      const yaml = safeDumpAll([{ a: '世界' }, { b: '🌍' }]);
      expect(yaml).toContain('世界');
      expect(yaml).toContain('🌍');
    });
  });

  describe('error cases for emitter', () => {
    it('should handle undefined in objects', () => {
      const yaml = safeDump({ key: undefined });
      expect(yaml).toBeTruthy();
    });

    it('should handle undefined in arrays', () => {
      const yaml = safeDump([1, undefined, 3]);
      expect(yaml).toBeTruthy();
    });

    it('should handle functions (convert to null or skip)', () => {
      const yaml = safeDump({ func: () => {} });
      expect(yaml).toBeTruthy();
    });

    it('should handle symbols (convert to null or skip)', () => {
      const yaml = safeDump({ sym: Symbol('test') });
      expect(yaml).toBeTruthy();
    });

    it('should handle Date objects', () => {
      const yaml = safeDump({ date: new Date('2024-01-01') });
      expect(yaml).toBeTruthy();
    });

    it('should handle RegExp objects', () => {
      const yaml = safeDump({ regex: /test/gi });
      expect(yaml).toBeTruthy();
    });

    it('should handle circular references gracefully', () => {
      const circular: any = { a: 1 };
      circular.self = circular;
      const yaml = safeDump(circular);
      expect(yaml).toBeTruthy();
    });
  });
});
