/**
 * Integration tests for fast-yaml-nodejs module
 */

import { describe, expect, it } from 'vitest';
import * as fastYaml from '../index';

describe('Module Exports', () => {
  it('should export all required functions', () => {
    expect(fastYaml.version).toBeDefined();
    expect(typeof fastYaml.version).toBe('function');

    expect(fastYaml.safeLoad).toBeDefined();
    expect(typeof fastYaml.safeLoad).toBe('function');

    expect(fastYaml.safeLoadAll).toBeDefined();
    expect(typeof fastYaml.safeLoadAll).toBe('function');

    expect(fastYaml.load).toBeDefined();
    expect(typeof fastYaml.load).toBe('function');

    expect(fastYaml.loadAll).toBeDefined();
    expect(typeof fastYaml.loadAll).toBe('function');

    expect(fastYaml.safeDump).toBeDefined();
    expect(typeof fastYaml.safeDump).toBe('function');

    expect(fastYaml.safeDumpAll).toBeDefined();
    expect(typeof fastYaml.safeDumpAll).toBe('function');
  });

  it('should export Mark class', () => {
    expect(fastYaml.Mark).toBeDefined();
    expect(typeof fastYaml.Mark).toBe('function');

    const mark = new fastYaml.Mark('test.yaml', 1, 5);
    expect(mark).toBeInstanceOf(fastYaml.Mark);
  });

  it('should export Schema enum', () => {
    expect(fastYaml.Schema).toBeDefined();
    expect(fastYaml.Schema.SafeSchema).toBeDefined();
    expect(fastYaml.Schema.JsonSchema).toBeDefined();
    expect(fastYaml.Schema.CoreSchema).toBeDefined();
    expect(fastYaml.Schema.FailsafeSchema).toBeDefined();
  });

  it('should have correct version format', () => {
    const v = fastYaml.version();
    expect(v).toBeTruthy();
    expect(typeof v).toBe('string');
    expect(v).toMatch(/^\d+\.\d+\.\d+/);
  });
});

describe('Module Integration', () => {
  it('should load and dump YAML successfully', () => {
    const data = { name: 'test', value: 123 };
    const yaml = fastYaml.safeDump(data);
    const parsed = fastYaml.safeLoad(yaml);
    expect(parsed).toEqual(data);
  });

  it('should work with all load/dump combinations', () => {
    const data = { key: 'value', num: 42 };

    const yaml1 = fastYaml.safeDump(data);
    expect(fastYaml.safeLoad(yaml1)).toEqual(data);
    expect(fastYaml.load(yaml1)).toEqual(data);

    const yaml2 = fastYaml.safeDump(data);
    expect(fastYaml.safeLoad(yaml2)).toEqual(data);
    expect(fastYaml.load(yaml2)).toEqual(data);
  });

  it('should work with multi-document YAML', () => {
    const docs = [{ a: 1 }, { b: 2 }, { c: 3 }];
    const yaml = fastYaml.safeDumpAll(docs);
    const parsed = fastYaml.safeLoadAll(yaml);
    expect(parsed).toEqual(docs);

    const parsed2 = fastYaml.loadAll(yaml);
    expect(parsed2).toEqual(docs);
  });

  it('should handle complex workflow', () => {
    const original = {
      server: {
        host: 'localhost',
        port: 8080,
        ssl: true,
      },
      database: {
        url: 'postgres://localhost/db',
        pool_size: 10,
      },
      features: ['auth', 'api', 'websocket'],
    };

    const yaml = fastYaml.safeDump(original, { sortKeys: true });
    expect(yaml).toContain('database:');
    expect(yaml).toContain('server:');

    const parsed = fastYaml.safeLoad(yaml);
    expect(parsed).toEqual(original);
  });

  it('should handle errors consistently across functions', () => {
    const invalidYaml = 'invalid: [unclosed';

    const result1 = fastYaml.safeLoad(invalidYaml);
    expect(result1).toBeInstanceOf(Error);

    const result2 = fastYaml.load(invalidYaml);
    expect(result2).toBeInstanceOf(Error);

    const result3 = fastYaml.safeLoadAll(invalidYaml);
    expect(result3).toBeInstanceOf(Error);

    const result4 = fastYaml.loadAll(invalidYaml);
    expect(result4).toBeInstanceOf(Error);
  });

  it('should handle empty input consistently', () => {
    expect(fastYaml.safeLoad('')).toBe(null);
    expect(fastYaml.load('')).toBe(null);
    expect(fastYaml.safeLoadAll('')).toEqual([]);
    expect(fastYaml.loadAll('')).toEqual([]);
  });

  it('should handle null and undefined consistently', () => {
    const yaml1 = fastYaml.safeDump(null);
    expect(yaml1.trim()).toBe('~');

    const yaml2 = fastYaml.safeDump({ key: null });
    expect(yaml2).toContain('key: ~');
  });

  it('should preserve types through round-trip', () => {
    const testCases = [
      { name: 'string', value: { s: 'text' } },
      { name: 'number', value: { n: 42 } },
      { name: 'float', value: { f: 3.14 } },
      { name: 'boolean', value: { b: true } },
      { name: 'null', value: { x: null } },
      { name: 'array', value: { a: [1, 2, 3] } },
      { name: 'object', value: { o: { nested: 'value' } } },
    ];

    testCases.forEach(({ value }) => {
      const yaml = fastYaml.safeDump(value);
      const parsed = fastYaml.safeLoad(yaml);
      expect(parsed).toEqual(value);
    });
  });
});

describe('Module Error Handling', () => {
  it('should handle size limit errors', () => {
    const large = 'x: '.repeat(35_000_000); // ~105MB, exceeds 100MB limit
    const result = fastYaml.safeLoad(large);
    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).toContain('exceeds maximum');
  });

  it('should handle malformed YAML errors', () => {
    // These are actual syntax errors in YAML
    const malformed = [
      'key: [unclosed',
      'key: {unclosed',
      'key:\n\tvalue', // tabs not allowed in indentation
    ];

    malformed.forEach((yaml) => {
      const result1 = fastYaml.safeLoad(yaml);
      expect(result1).toBeInstanceOf(Error);

      const result2 = fastYaml.load(yaml);
      expect(result2).toBeInstanceOf(Error);
    });
  });

  it('should parse valid YAML that looks unusual', () => {
    // 'key value' is valid YAML - it's a plain string
    expect(fastYaml.safeLoad('key value')).toBe('key value');
  });

  it('should provide meaningful error messages', () => {
    const result = fastYaml.safeLoad('key: [unclosed');
    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).toBeTruthy();
    expect((result as Error).message.length).toBeGreaterThan(10);
  });
});

describe('Module Performance Characteristics', () => {
  it('should handle moderately large documents', () => {
    const largeDoc = {
      items: Array(1000)
        .fill(null)
        .map((_, i) => ({ id: i, name: `item${i}`, active: i % 2 === 0 })),
    };

    const yaml = fastYaml.safeDump(largeDoc);
    expect(yaml).toBeTruthy();

    const parsed = fastYaml.safeLoad(yaml);
    expect(parsed).toEqual(largeDoc);
  });

  it('should handle many small documents', () => {
    const docs = Array(100)
      .fill(null)
      .map((_, i) => ({ index: i, value: `doc${i}` }));

    const yaml = fastYaml.safeDumpAll(docs);
    expect(yaml).toBeTruthy();

    const parsed = fastYaml.safeLoadAll(yaml);
    expect(parsed).toEqual(docs);
  });

  it('should handle deeply nested structures', () => {
    let deep: Record<string, unknown> = { value: 'end' };
    for (let i = 0; i < 20; i++) {
      deep = { level: deep };
    }

    const yaml = fastYaml.safeDump(deep);
    const parsed = fastYaml.safeLoad(yaml);
    expect(parsed).toBeTruthy();
  });
});
