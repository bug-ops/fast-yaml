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

  it('returns error on invalid YAML', () => {
    const yaml = '---\nfoo: bar\n---\n{ invalid: yaml: structure ]';
    const result = parseParallel(yaml);
    expect(result).toBeInstanceOf(Error);
    expect((result as Error).message).toMatch(/parse|invalid|error/i);
  });

  it('validates config limits', () => {
    const yaml = 'foo: bar';

    // Thread count too high
    const result1 = parseParallel(yaml, { threadCount: 1000 });
    expect(result1).toBeInstanceOf(Error);
    expect((result1 as Error).message).toMatch(/threadCount|thread|128/i);

    // Invalid chunk sizes - max < min
    const result2 = parseParallel(yaml, {
      minChunkSize: 10000,
      maxChunkSize: 1000,
    });
    expect(result2).toBeInstanceOf(Error);
    expect((result2 as Error).message).toMatch(/chunk|size/i);
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

    // Invalid chunk sizes - max < min
    await expect(
      parseParallelAsync(yaml, {
        minChunkSize: 10000,
        maxChunkSize: 1000,
      })
    ).rejects.toThrow(/chunk|size/i);
  });

  it('returns error on invalid YAML in async mode', async () => {
    const yaml = '---\nfoo: bar\n---\n{ invalid: yaml: structure ]';
    await expect(parseParallelAsync(yaml)).rejects.toThrow(/parse|invalid|error/i);
  });
});
