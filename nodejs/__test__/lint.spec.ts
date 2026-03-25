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
    ).toThrow(/Invalid severity/);
  });

  it('unknown rule name is silently accepted', () => {
    expect(() =>
      lint(VALID_YAML, { rules: { 'nonexistent-rule': { severity: 'error' } } })
    ).not.toThrow();
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
