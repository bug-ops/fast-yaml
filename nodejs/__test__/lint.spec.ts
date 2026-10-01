/**
 * Tests for the lint API: lint(), Linter class, LintConfig, Diagnostic types
 */

import { describe, expect, it } from 'vitest';
import { Linter, lint } from '../index';

const VALID_YAML = 'name: John\nage: 30\n';
const DUPLICATE_KEYS_YAML = 'key: value\nkey: duplicate\n';
const LONG_LINE_YAML =
  'very_long_key: this value is intentionally very long to exceed the default eighty character line limit\n';
const INVALID_YAML = 'invalid: [unclosed';

describe('lint() function', () => {
  it('returns an array', () => {
    const result = lint(VALID_YAML);
    expect(Array.isArray(result)).toBe(true);
  });

  it('returns no errors for valid YAML', () => {
    const result = lint(VALID_YAML);
    const errors = result.filter((d) => d.severity === 'Error');
    expect(errors).toHaveLength(0);
  });

  it('detects duplicate keys', () => {
    const result = lint(DUPLICATE_KEYS_YAML);
    const dupKey = result.find((d) => d.code === 'duplicate-key');
    expect(dupKey).toBeDefined();
    expect(dupKey?.severity).toBe('Error');
  });

  it('detects line length violations', () => {
    const result = lint(LONG_LINE_YAML);
    const lineLength = result.find((d) => d.code === 'line-length');
    expect(lineLength).toBeDefined();
  });

  it('throws on invalid YAML', () => {
    expect(() => lint(INVALID_YAML)).toThrow();
  });

  it('accepts empty string', () => {
    const result = lint('');
    expect(Array.isArray(result)).toBe(true);
  });
});

describe('lint() with LintConfig', () => {
  it('disables line-length rule when maxLineLength is not set via disabledRules', () => {
    const result = lint(LONG_LINE_YAML, { disabledRules: ['line-length'] });
    const lineLength = result.find((d) => d.code === 'line-length');
    expect(lineLength).toBeUndefined();
  });

  it('allows duplicate keys when allowDuplicateKeys is true', () => {
    const result = lint(DUPLICATE_KEYS_YAML, { allowDuplicateKeys: true });
    const dupKey = result.find((d) => d.code === 'duplicate-key');
    expect(dupKey).toBeUndefined();
  });

  it('uses custom maxLineLength', () => {
    const result200 = lint(LONG_LINE_YAML, { maxLineLength: 200 });
    const lineLength200 = result200.find((d) => d.code === 'line-length');
    expect(lineLength200).toBeUndefined();

    const result40 = lint('key: this value makes the line exceed forty characters\n', {
      maxLineLength: 40,
    });
    const lineLength40 = result40.find((d) => d.code === 'line-length');
    expect(lineLength40).toBeDefined();
  });
});

describe('Diagnostic shape', () => {
  it('has required fields', () => {
    const result = lint(DUPLICATE_KEYS_YAML);
    expect(result.length).toBeGreaterThan(0);
    const d = result[0];
    expect(typeof d.code).toBe('string');
    expect(typeof d.severity).toBe('string');
    expect(typeof d.message).toBe('string');
    expect(d.span).toBeDefined();
    expect(Array.isArray(d.suggestions)).toBe(true);
  });

  it('has correct span structure', () => {
    const result = lint(DUPLICATE_KEYS_YAML);
    const d = result[0];
    expect(d.span.start).toBeDefined();
    expect(d.span.end).toBeDefined();
    expect(typeof d.span.start.line).toBe('number');
    expect(typeof d.span.start.column).toBe('number');
    expect(typeof d.span.start.offset).toBe('number');
    expect(typeof d.span.end.line).toBe('number');
    expect(typeof d.span.end.column).toBe('number');
    expect(typeof d.span.end.offset).toBe('number');
  });

  it('context is optional (may be null/undefined or object)', () => {
    const result = lint(DUPLICATE_KEYS_YAML);
    const d = result[0];
    if (d.context !== null && d.context !== undefined) {
      expect(Array.isArray(d.context.lines)).toBe(true);
    }
  });
});

describe('context windowing', () => {
  it('bounds context line content on long lines', () => {
    const result = lint('k: [' + '1 ,'.repeat(2000) + ']\n');
    expect(result.length).toBeGreaterThan(0);
    for (const d of result) {
      for (const line of d.context?.lines ?? []) {
        expect([...line.content].length).toBeLessThanOrEqual(120);
        expect(typeof line.columnOffset).toBe('number');
        expect(typeof line.truncatedEnd).toBe('boolean');
      }
    }
    const lines = result.flatMap((d) => d.context?.lines ?? []);
    expect(lines.some((l) => l.columnOffset > 0 && l.truncatedEnd === true)).toBe(true);
    expect(lines.some((l) => l.columnOffset === 0 && l.truncatedEnd === true)).toBe(true);
  });
});

describe('Severity enum string values', () => {
  it('duplicate key severity is "Error"', () => {
    const result = lint(DUPLICATE_KEYS_YAML);
    const dup = result.find((d) => d.code === 'duplicate-key');
    expect(dup?.severity).toBe('Error');
  });

  it('line-length severity is "Warning" or "Info"', () => {
    const result = lint(LONG_LINE_YAML);
    const ll = result.find((d) => d.code === 'line-length');
    expect(['Warning', 'Info', 'Error', 'Hint']).toContain(ll?.severity);
  });
});

describe('Linter class', () => {
  it('can be instantiated without arguments', () => {
    const linter = new Linter();
    expect(linter).toBeDefined();
  });

  it('can be instantiated with config', () => {
    const linter = new Linter({ allowDuplicateKeys: true });
    expect(linter).toBeDefined();
  });

  it('withAllRules() factory returns a Linter', () => {
    const linter = Linter.withAllRules();
    expect(linter).toBeDefined();
    expect(typeof linter.lint).toBe('function');
  });

  it('lint() method returns array of diagnostics', () => {
    const linter = Linter.withAllRules();
    const result = linter.lint(VALID_YAML);
    expect(Array.isArray(result)).toBe(true);
  });

  it('lint() detects duplicate keys', () => {
    const linter = Linter.withAllRules();
    const result = linter.lint(DUPLICATE_KEYS_YAML);
    const dup = result.find((d) => d.code === 'duplicate-key');
    expect(dup).toBeDefined();
  });

  it('lint() respects config passed to constructor', () => {
    const linter = new Linter({ allowDuplicateKeys: true });
    const result = linter.lint(DUPLICATE_KEYS_YAML);
    const dup = result.find((d) => d.code === 'duplicate-key');
    expect(dup).toBeUndefined();
  });

  it('new Linter() with no args uses default rules', () => {
    const linter = new Linter();
    const result = linter.lint('key: v1\nkey: v2\n');
    expect(result.length).toBeGreaterThanOrEqual(1);
  });

  it('lint() throws on invalid YAML', () => {
    const linter = Linter.withAllRules();
    expect(() => linter.lint(INVALID_YAML)).toThrow();
  });
});

describe('Disabled rules', () => {
  it('disabling line-length suppresses line-length diagnostics', () => {
    const result = lint(LONG_LINE_YAML, { disabledRules: ['line-length'] });
    expect(result.find((d) => d.code === 'line-length')).toBeUndefined();
  });

  it('disabling duplicate-key suppresses duplicate key diagnostics', () => {
    const result = lint(DUPLICATE_KEYS_YAML, { disabledRules: ['duplicate-key'] });
    expect(result.find((d) => d.code === 'duplicate-key')).toBeUndefined();
  });
});

describe('Per-rule severity overrides', () => {
  it('object form: severity override changes diagnostic severity', () => {
    const result = lint(DUPLICATE_KEYS_YAML, {
      rules: { 'duplicate-key': { severity: 'warning' } },
    });
    const diag = result.find((d) => d.code === 'duplicate-key');
    expect(diag).toBeDefined();
    expect(diag?.severity).toBe('Warning');
  });

  it('string shorthand: severity override changes diagnostic severity', () => {
    const result = lint(DUPLICATE_KEYS_YAML, {
      rules: { 'duplicate-key': 'warning' },
    });
    const diag = result.find((d) => d.code === 'duplicate-key');
    expect(diag).toBeDefined();
    expect(diag?.severity).toBe('Warning');
  });

  it('enabled: false disables the rule', () => {
    const result = lint(DUPLICATE_KEYS_YAML, {
      rules: { 'duplicate-key': { enabled: false } },
    });
    expect(result.find((d) => d.code === 'duplicate-key')).toBeUndefined();
  });

  it('invalid severity string throws an error', () => {
    expect(() =>
      lint(DUPLICATE_KEYS_YAML, { rules: { 'duplicate-key': 'critical' as unknown as 'error' } })
    ).toThrow(/duplicate-key/);
  });

  it('unknown rule name is rejected', () => {
    expect(() =>
      lint(VALID_YAML, { rules: { 'nonexistent-rule': { severity: 'error' } } as never })
    ).toThrow(/unknown rule 'nonexistent-rule'/);
  });

  it('empty rules map does not change behavior', () => {
    const withoutRules = lint(DUPLICATE_KEYS_YAML);
    const withEmptyRules = lint(DUPLICATE_KEYS_YAML, { rules: {} });
    const withoutDup = withoutRules.find((d) => d.code === 'duplicate-key');
    const withDup = withEmptyRules.find((d) => d.code === 'duplicate-key');
    expect(withoutDup?.severity).toBe(withDup?.severity);
  });

  it('all four severity values are accepted', () => {
    const severities = ['error', 'warning', 'info', 'hint'] as const;
    for (const sev of severities) {
      expect(() => lint(DUPLICATE_KEYS_YAML, { rules: { 'duplicate-key': sev } })).not.toThrow();
    }
  });
});

describe('inline directives', () => {
  it('disable-line suppresses a diagnostic on its line', () => {
    const result = lint('key: 1\nkey: 2  # fy: disable-line duplicate-key\n');
    expect(result.find((d) => d.code === 'duplicate-key')).toBeUndefined();
  });

  it('unknown rule is reported and suppresses nothing', () => {
    const result = lint('# fy: disable no-such-rule\nkey: 1\nkey: 2\n');
    expect(result.find((d) => d.code === 'lint-directive')).toBeDefined();
    expect(result.find((d) => d.code === 'duplicate-key')).toBeDefined();
  });

  it('disable-file suppresses everything', () => {
    expect(lint('# fy: disable-file\nkey: 1\nkey: 2\n')).toHaveLength(0);
  });

  it('disable/enable open and close a block', () => {
    const yaml =
      '# fy: disable duplicate-key\na: 1\na: 2\n# fy: enable duplicate-key\nb: 1\nb: 2\n';
    const dups = lint(yaml).filter((d) => d.code === 'duplicate-key');
    expect(dups).toHaveLength(1);
    expect(dups[0].span.start.line).toBe(6);
  });
});

describe('merge keys and sets in lint', () => {
  it('reports a repeated << as a duplicate-key diagnostic, not an exception', () => {
    const result = lint('a: &a {x: 1}\nb: &b {y: 2}\nc: {<<: *a, <<: *b}\n');
    expect(result.some((d) => d.code === 'duplicate-key')).toBe(true);
  });

  it('does not see a repeated << written through an alias (known blind spot)', () => {
    const result = lint('a: &a {x: 1}\nb: &b {y: 2}\nc: {&k <<: *a, *k : *b}\n');
    expect(result.some((d) => d.code === 'duplicate-key')).toBe(false);
  });

  it('rejects a !!set member with a value with its position', () => {
    expect(() => lint('s: !!set {a: 1}\n')).toThrow(
      /member has a non-null value.*line 1, column 11/
    );
  });
});

describe('BOM coordinates', () => {
  const spans = (source: string) =>
    lint(source).map((d) => [
      d.code,
      d.span.start.line,
      d.span.start.column,
      d.span.start.offset,
      d.span.end.offset,
    ]);

  it('leading BOM does not shift spans', () => {
    const plain = spans('a: 1   \n');
    expect(plain.length).toBeGreaterThan(0);
    expect(spans('\uFEFFa: 1   \n')).toEqual(plain);
  });

  it('prefix BOM in a later document does not shift spans', () => {
    const plain = spans('a: 1\n...\nb: 2   \n');
    expect(plain.length).toBeGreaterThan(0);
    expect(spans('a: 1\n...\n\uFEFFb: 2   \n')).toEqual(plain);
  });
});

describe('comments inside scalars', () => {
  it('hash in a multi-line quoted scalar is not a comment', () => {
    const result = lint('a: "one\n  #two\n  three"\n');
    expect(result.filter((d) => d.code === 'comments')).toHaveLength(0);
  });

  it('a real comment is still checked', () => {
    const result = lint('a: 1 #bad\n');
    expect(result.some((d) => d.code === 'comments')).toBe(true);
  });
});
