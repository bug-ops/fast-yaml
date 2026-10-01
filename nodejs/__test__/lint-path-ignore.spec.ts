/**
 * Per-rule `ignore` with a `path` argument, and yamllint rule names (#585, #589).
 */

import { mkdirSync, mkdtempSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { type LintConfig, Linter, lint } from '../index';

const SOURCE = 'a: 1 \nb: 2\nb: 3\n';
const codes = (diagnostics: { code: string }[]) => diagnostics.map((d) => d.code);

describe('per-rule ignore and path', () => {
  const original = process.cwd();
  let dir: string;

  beforeEach(() => {
    dir = realpathSync(mkdtempSync(join(tmpdir(), 'fy-path-ignore-')));
    mkdirSync(join(dir, 'src'));
    mkdirSync(join(dir, 'generated'));
    process.chdir(dir);
  });

  afterEach(() => {
    process.chdir(original);
  });

  const config: LintConfig = { rules: { 'trailing-whitespace': { ignore: 'generated/' } } };

  it('skips the rule for a matching path only', () => {
    expect(codes(lint(SOURCE, config, join(dir, 'src', 'a.yaml')))).toContain(
      'trailing-whitespace',
    );
    const skipped = codes(lint(SOURCE, config, join(dir, 'generated', 'a.yaml')));
    expect(skipped).not.toContain('trailing-whitespace');
    expect(skipped).toContain('duplicate-key');
  });

  it('never ignores a source without a path', () => {
    expect(codes(lint(SOURCE, config))).toContain('trailing-whitespace');
  });

  it('accepts relative paths and the Linter method', () => {
    const linter = new Linter(config);
    expect(codes(linter.lint(SOURCE, 'generated/a.yaml'))).not.toContain('trailing-whitespace');
    expect(codes(linter.lint(SOURCE, 'src/a.yaml'))).toContain('trailing-whitespace');
    expect(codes(linter.lint(SOURCE))).toContain('trailing-whitespace');
  });

  it('reads ignore-from-file relative to the working directory', () => {
    writeFileSync(join(dir, 'ignores'), 'generated/\n');
    const fromFile: LintConfig = {
      rules: { 'trailing-whitespace': { 'ignore-from-file': 'ignores' } },
    };
    expect(codes(lint(SOURCE, fromFile, 'generated/a.yaml'))).not.toContain('trailing-whitespace');
  });

  it('rejects ignore combined with ignore-from-file', () => {
    expect(() =>
      lint(SOURCE, { rules: { braces: { ignore: ['x/'], 'ignore-from-file': 'ignores' } } }),
    ).toThrow(/cannot be used together/);
  });

  it('rejects a path in a missing directory', () => {
    expect(() => lint(SOURCE, undefined, 'no-such-dir/a.yaml')).toThrow(/cannot resolve path/);
  });
});

describe('yamllint rule names and enable', () => {
  it('accepts yamllint names as rule keys and in disabledRules', () => {
    const found = lint(SOURCE, {
      rules: { 'trailing-spaces': 'disable', 'key-duplicates': 'warning' },
      disabledRules: ['anchors'],
    });
    expect(codes(found)).not.toContain('trailing-whitespace');
    expect(found.filter((d) => d.code === 'duplicate-key').map((d) => d.severity)).toEqual([
      'Warning',
    ]);
  });

  it('enable resets to the yamllint defaults', () => {
    expect(codes(lint('a: 1\n', { rules: { 'document-start': 'enable' } }))).toContain(
      'document-start',
    );
  });
});
