import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { formatFiles, safeDump, safeDumpAll } from '../index.js';

describe('indent and width are validated, not clamped', () => {
  it.each([
    [{ indent: 0 }, /indent must be between 1 and 9, got 0/],
    [{ indent: 10 }, /indent must be between 1 and 9, got 10/],
    [{ width: 19 }, /width must be between 20 and 1000, got 19/],
    [{ width: 1001 }, /width must be between 20 and 1000, got 1001/],
  ])('safeDump rejects %j', (options, message) => {
    expect(() => safeDump({ a: 1 }, options)).toThrow(message);
    expect(() => safeDumpAll([{ a: 1 }], options)).toThrow(message);
  });

  it('accepts the range limits', () => {
    expect(safeDump({ a: { b: 1 } }, { indent: 1, width: 20 })).toBe('a:\n b: 1\n');
    expect(safeDump({ a: { b: 1 } }, { indent: 9, width: 1000 })).toBe('a:\n         b: 1\n');
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

  it('rejects out-of-range indent and width', () => {
    expect(() => formatFiles([file], { indent: 0 })).toThrow(/indent must be between 1 and 9/);
    expect(() => formatFiles([file], { width: 5 })).toThrow(/width must be between 20 and 1000/);
  });

  it('honors maxDepth', () => {
    const [low] = formatFiles([file], { maxDepth: 2 });
    expect(low.content).toBeUndefined();
    expect(low.error).toMatch(/nesting depth exceeds 2/);
    const [ok] = formatFiles([file], { maxDepth: 3 });
    expect(ok.error).toBeUndefined();
    expect(ok.content).toContain('1');
  });
});
