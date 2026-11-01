import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { formatFiles, processFiles, safeDump, safeDumpAll } from '../index.js';

describe('indent is validated, not clamped', () => {
  it.each([
    [{ indent: 0 }, /indent must be between 1 and 9, got 0/],
    [{ indent: 10 }, /indent must be between 1 and 9, got 10/],
  ])('safeDump rejects %j', (options, message) => {
    expect(() => safeDump({ a: 1 }, options)).toThrow(message);
    expect(() => safeDumpAll([{ a: 1 }], options)).toThrow(message);
  });

  it('accepts the range limits', () => {
    expect(safeDump({ a: { b: 1 } }, { indent: 1 })).toBe('a:\n b: 1\n');
    expect(safeDump({ a: { b: 1 } }, { indent: 9 })).toBe('a:\n         b: 1\n');
  });
});

describe('removed options are rejected, not ignored', () => {
  it.each([
    [{ width: 80 }, /width was removed/],
    [{ allowUnicode: true }, /allowUnicode was removed/],
  ])('safeDump rejects %j', (options, message) => {
    expect(() => safeDump({ a: 1 }, options as never)).toThrow(message);
    expect(() => safeDumpAll([{ a: 1 }], options as never)).toThrow(message);
  });

  it.each([
    [{ width: 80 }, /width was removed/],
    [{ sortKeys: true }, /sortKeys was removed/],
  ])('batch functions reject %j', (config, message) => {
    expect(() => processFiles([], config as never)).toThrow(message);
    expect(() => formatFiles([], config as never)).toThrow(message);
  });
});

describe('formatFiles options', () => {
  let tmpDir: string;
  let file: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'emitter-ranges-'));
    file = path.join(tmpDir, 'nested.yaml');
    fs.writeFileSync(file, '[[[1]]]\n');
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('rejects out-of-range indent', () => {
    expect(() => formatFiles([file], { indent: 0 })).toThrow(/indent must be between 1 and 9/);
  });

  it('honors maxDepth', () => {
    const [low] = formatFiles([file], { maxDepth: 2 });
    expect(low.status).toBe('failed');
    if (low.status === 'failed') {
      expect(low.error.kind).toBe('limit');
      expect(low.error.message).toMatch(/nesting depth exceeds 2/);
    }
    const [ok] = formatFiles([file], { maxDepth: 3 });
    expect(ok.status).toBe('formatted');
    if (ok.status === 'formatted') {
      expect(ok.content).toContain('1');
    }
  });
});
