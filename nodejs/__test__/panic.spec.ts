/**
 * Panic containment tests. Require a binary built with the `test-panic` feature:
 * `pnpm run build:test && pnpm run test:panic` (release builds do not enable it).
 */

import { describe, expect, it } from 'vitest';
import * as binding from '../index';

const exports = binding as Record<string, unknown>;
const hasPanicExports =
  typeof exports.testPanic === 'function' && typeof exports.testPanicAsync === 'function';

if (process.env.CI && !hasPanicExports) {
  throw new Error('CI build is missing the test-panic feature exports (use `pnpm run build:test`)');
}

describe.skipIf(!hasPanicExports)('panic containment', () => {
  it('throws a catchable Error for a panic in a sync export', () => {
    const testPanic = exports.testPanic as () => string;
    expect(() => testPanic()).toThrow(/test-panic: intentional panic/);
  });

  it('rejects the promise for a panic on the async worker thread', async () => {
    const testPanicAsync = exports.testPanicAsync as () => Promise<void>;
    await expect(testPanicAsync()).rejects.toThrow(/intentional async panic/);
  });

  it('keeps serving calls after panics', () => {
    const testPanic = exports.testPanic as () => string;
    for (let i = 0; i < 3; i++) {
      expect(() => testPanic()).toThrow();
    }
    expect(binding.safeLoad('a: 1')).toEqual({ a: 1 });
    expect(binding.version()).toMatch(/^\d+\.\d+\.\d+/);
  });
});
