import { bench, describe } from 'vitest';
import { safeDump, safeLoad } from '../index';

describe('Pre-allocation Optimizations', () => {
  // Benchmark large array conversion (10K elements)
  bench('safeLoad - Large array (10K elements)', () => {
    const arrayYaml = Array.from({ length: 10000 }, (_, i) => `- item${i}`).join('\n');
    const result = safeLoad(arrayYaml);
    if (!Array.isArray(result) || result.length !== 10000) {
      throw new Error('Expected array with 10000 elements');
    }
  });

  bench('safeDump - Large array (10K elements)', () => {
    const largeArray = Array.from({ length: 10000 }, (_, i) => ({ id: i, name: `item${i}` }));
    const yaml = safeDump(largeArray);
    if (!yaml || yaml.length === 0) {
      throw new Error('Expected non-empty YAML output');
    }
  });

  // Benchmark large object conversion (10K properties)
  bench('safeLoad - Large object (10K properties)', () => {
    const objectYaml = Array.from({ length: 10000 }, (_, i) => `key${i}: value${i}`).join('\n');
    const result = safeLoad(objectYaml);
    if (typeof result !== 'object' || result === null) {
      throw new Error('Expected object');
    }
  });

  bench('safeDump - Large object (10K properties)', () => {
    const largeObject = Object.fromEntries(
      Array.from({ length: 10000 }, (_, i) => [`key${i}`, `value${i}`])
    );
    const yaml = safeDump(largeObject);
    if (!yaml || yaml.length === 0) {
      throw new Error('Expected non-empty YAML output');
    }
  });

  // Benchmark nested structure (1MB+ serialized)
  bench('safeDump - Deeply nested structure', () => {
    const createNestedStructure = (depth: number): any => {
      if (depth === 0) {
        return Array.from({ length: 100 }, (_, i) => ({ id: i, value: `data${i}` }));
      }
      return {
        level: depth,
        data: Array.from({ length: 10 }, () => createNestedStructure(depth - 1)),
      };
    };

    const nested = createNestedStructure(3);
    const yaml = safeDump(nested);
    if (!yaml || yaml.length === 0) {
      throw new Error('Expected non-empty YAML output');
    }
  });

  // Benchmark round-trip for large data
  bench('Round-trip - Large array (1K elements)', () => {
    const data = Array.from({ length: 1000 }, (_, i) => ({
      id: i,
      name: `user${i}`,
      email: `user${i}@example.com`,
      age: 20 + (i % 50),
      active: i % 2 === 0,
    }));

    const yaml = safeDump(data);
    const parsed = safeLoad(yaml);

    if (!Array.isArray(parsed) || parsed.length !== 1000) {
      throw new Error('Round-trip failed');
    }
  });

  // Verify linear scaling for arrays (should not be O(n^2))
  bench('safeDump - Array scaling: 1K elements', () => {
    const array = Array.from({ length: 1000 }, (_, i) => i);
    safeDump(array);
  });

  bench('safeDump - Array scaling: 5K elements', () => {
    const array = Array.from({ length: 5000 }, (_, i) => i);
    safeDump(array);
  });

  bench('safeDump - Array scaling: 10K elements', () => {
    const array = Array.from({ length: 10000 }, (_, i) => i);
    safeDump(array);
  });

  // Verify linear scaling for objects (should not be O(n^2))
  bench('safeDump - Object scaling: 1K properties', () => {
    const obj = Object.fromEntries(
      Array.from({ length: 1000 }, (_, i) => [`key${i}`, i])
    );
    safeDump(obj);
  });

  bench('safeDump - Object scaling: 5K properties', () => {
    const obj = Object.fromEntries(
      Array.from({ length: 5000 }, (_, i) => [`key${i}`, i])
    );
    safeDump(obj);
  });

  bench('safeDump - Object scaling: 10K properties', () => {
    const obj = Object.fromEntries(
      Array.from({ length: 10000 }, (_, i) => [`key${i}`, i])
    );
    safeDump(obj);
  });
});
