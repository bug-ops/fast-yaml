import { describe, expect, it } from 'vitest';
import { parseParallel, parseParallelAsync } from '../index.js';

describe('parseParallel', () => {
  it('parses single document', () => {
    const yaml = 'foo: 1\nbar: 2';
    const docs = parseParallel(yaml);
    expect(docs).toHaveLength(1);
    expect(docs[0]).toEqual({ foo: 1, bar: 2 });
  });

  it('parses multi-document YAML', () => {
    const yaml = '---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3';
    const docs = parseParallel(yaml);
    expect(docs).toHaveLength(3);
    expect(docs[0]).toEqual({ foo: 1 });
    expect(docs[1]).toEqual({ bar: 2 });
    expect(docs[2]).toEqual({ baz: 3 });
  });

  it('handles empty input', () => {
    const docs = parseParallel('');
    expect(docs).toHaveLength(0);
  });

  it('respects config options', () => {
    const yaml = '---\nfoo: 1\n---\nbar: 2';
    const config = {
      threadCount: 2,
      minChunkSize: 1024,
    };
    const docs = parseParallel(yaml, config);
    expect(docs).toHaveLength(2);
  });

  it('throws on invalid YAML', () => {
    const yaml = '---\nfoo: bar\n---\n{ invalid: yaml: structure ]';
    expect(() => parseParallel(yaml)).toThrow(/parse|invalid|error/i);
  });

  it('validates config limits', () => {
    const yaml = 'foo: bar';

    // Thread count too high
    expect(() => parseParallel(yaml, { threadCount: 1000 })).toThrow(/threadCount|thread|128/i);

    expect(() => parseParallel(yaml, { minChunkSize: 0 })).toThrow(/minChunkSize/);
  });
});

describe('parseParallel limits', () => {
  const docs = (n: number) => '---\na: 1\n'.repeat(n);

  it('enforces maxDocuments', () => {
    const tooMany = /at least \d+ documents, more than the maximum of 2/;
    expect(() => parseParallel(docs(3), { maxDocuments: 2 })).toThrow(tooMany);
    expect(parseParallel(docs(2), { maxDocuments: 2 })).toHaveLength(2);
  });

  it('enforces the default document limit without a config', () => {
    expect(() => parseParallel(docs(100_001))).toThrow(/maximum of 100000/);
  });

  it('enforces maxInputBytes', () => {
    expect(() => parseParallel(docs(3), { maxInputBytes: 8 })).toThrow(/exceeds|too large|limit/i);
  });

  it('keeps !!set output as a plain object', () => {
    expect(parseParallel('--- !!set {a, b}\n')).toEqual([{ a: null, b: null }]);
  });

  it('enforces maxDocuments in async mode', async () => {
    await expect(parseParallelAsync(docs(3), { maxDocuments: 2 })).rejects.toThrow(/maximum of 2/);
  });
});

describe('parseParallelAsync', () => {
  it('parses multi-document YAML', async () => {
    const yaml = '---\nfoo: 1\n---\nbar: 2';
    const docs = await parseParallelAsync(yaml);
    expect(docs).toHaveLength(2);
    expect(docs[0]).toEqual({ foo: 1 });
    expect(docs[1]).toEqual({ bar: 2 });
  });

  it('handles empty input', async () => {
    const docs = await parseParallelAsync('');
    expect(docs).toHaveLength(0);
  });

  it('respects config options', async () => {
    const yaml = '---\na: 1\n---\nb: 2\n---\nc: 3';
    const config = { threadCount: 4 };
    const docs = await parseParallelAsync(yaml, config);
    expect(docs).toHaveLength(3);
  });

  it('validates config limits in async mode', async () => {
    const yaml = 'foo: bar';

    // Thread count too high
    await expect(parseParallelAsync(yaml, { threadCount: 1000 })).rejects.toThrow(
      /threadCount|thread|128/i
    );

    await expect(parseParallelAsync(yaml, { minChunkSize: 0 })).rejects.toThrow(/minChunkSize/);
  });

  it('returns error on invalid YAML in async mode', async () => {
    const yaml = '---\nfoo: bar\n---\n{ invalid: yaml: structure ]';
    await expect(parseParallelAsync(yaml)).rejects.toThrow(/parse|invalid|error/i);
  });
});
