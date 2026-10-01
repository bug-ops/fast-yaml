/**
 * Tests for typed per-rule options in LintConfig.rules and parity with `fy lint --config`.
 */

import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { type LintConfig, Linter, lint } from '../index';

const codes = (source: string, config?: LintConfig) => lint(source, config).map((d) => d.code);
const bad = (rules: unknown) => ({ rules }) as unknown as LintConfig;

describe('rule options pass-through', () => {
  it('quoted-strings quote-type', () => {
    const source = 'a: "x y"\n';
    const rules = (quoteType: 'single' | 'double') => ({
      rules: { 'quoted-strings': { 'quote-type': quoteType, required: true } },
    });
    expect(codes(source, rules('single'))).toContain('quoted-strings');
    expect(codes(source, rules('double'))).not.toContain('quoted-strings');
  });

  it('document-start present as bool', () => {
    expect(codes('a: 1\n', { rules: { 'document-start': { present: true } } })).toContain(
      'document-start'
    );
    expect(codes('---\na: 1\n', { rules: { 'document-start': { present: true } } })).not.toContain(
      'document-start'
    );
  });

  it('line-length max', () => {
    const source = 'key: this value makes the line exceed forty characters\n';
    expect(codes(source, { rules: { 'line-length': { max: 40 } } })).toContain('line-length');
    expect(codes(source, { rules: { 'line-length': { max: 200 } } })).not.toContain('line-length');
  });

  it('line-length max null removes the limit', () => {
    const source = `key: ${'x'.repeat(200)}\n`;
    expect(codes(source)).toContain('line-length');
    expect(codes(source, { rules: { 'line-length': { max: null } } })).not.toContain('line-length');
  });

  it('works through the Linter class', () => {
    const linter = new Linter({ rules: { 'line-length': { max: 10 } } });
    expect(linter.lint('key: longer than ten\n').map((d) => d.code)).toContain('line-length');
  });
});

describe('application order', () => {
  it('option fields override the rules patch', () => {
    const source = 'key: this value makes the line exceed forty characters\n';
    const config: LintConfig = { maxLineLength: 200, rules: { 'line-length': { max: 40 } } };
    expect(codes(source, config)).not.toContain('line-length');
    expect(codes(source, { rules: { 'line-length': { max: 40 } } })).toContain('line-length');
    expect(
      codes(source, { maxLineLength: undefined, rules: { 'line-length': { max: 40 } } })
    ).toContain('line-length');
  });

  it('indentSize overrides rules.indentation.spaces', () => {
    const source = 'a:\n    b: 1\n';
    const rules = { indentation: { spaces: 4 } };
    expect(codes(source, { rules })).not.toContain('indentation');
    expect(codes(source, { indentSize: 2, rules })).toContain('indentation');
  });

  it('disabledRules wins over rules enabled', () => {
    const config: LintConfig = {
      rules: { 'duplicate-key': { enabled: true } },
      disabledRules: ['duplicate-key'],
    };
    expect(codes('a: 1\na: 2\n', config)).not.toContain('duplicate-key');
  });

  it('allowDuplicateKeys wins over a rules entry that enables the rule', () => {
    const config: LintConfig = {
      allowDuplicateKeys: true,
      rules: { 'duplicate-key': { enabled: true } },
    };
    expect(codes('a: 1\na: 2\n', config)).not.toContain('duplicate-key');
  });

  it('maxLineLength unset keeps the default limit', () => {
    expect(codes(`key: ${'x'.repeat(200)}\n`, {})).toContain('line-length');
  });
});

describe('config errors', () => {
  it('typo in enum option names rule and key', () => {
    expect(() => lint('a: 1\n', bad({ 'quoted-strings': { 'quote-type': 'singel' } }))).toThrow(
      /rule 'quoted-strings', option 'quote-type'.*singel/
    );
  });

  it('unknown option key', () => {
    expect(() => lint('a: 1\n', bad({ colons: { 'max-space-after': 1 } }))).toThrow(/colons/);
  });

  it('unknown rule without a yamllint alias has no hint', () => {
    let message = '';
    try {
      lint('a: 1\n', bad({ 'no-such-rule': 'enable' }));
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toContain("unknown rule 'no-such-rule'");
    expect(message).not.toContain('yamllint');
  });

  it('unknown rule', () => {
    expect(() => lint('a: 1\n', bad({ 'no-such-rule': 'error' }))).toThrow(
      /unknown rule 'no-such-rule'/
    );
  });

  it('wrong value type', () => {
    expect(() => lint('a: 1\n', bad({ 'line-length': { max: 'wide' } }))).toThrow(
      /rule 'line-length', option 'max'/
    );
  });

  it('null option value is rejected except line-length max', () => {
    expect(() => lint('a: 1\n', bad({ colons: { 'max-spaces-after': null } }))).toThrow(
      /max-spaces-after/
    );
    expect(() => lint('a: 1\n', bad({ 'line-length': { max: null } }))).not.toThrow();
  });

  it('non-object rules is rejected', () => {
    expect(() => lint('a: 1\n', bad(['error']))).toThrow();
  });

  it.each([5, 'error', true, []])('non-object rules %j names the rules field', (rules) => {
    expect(() => lint('a: 1\n', bad(rules))).toThrow(/^.*rules must be an object.*got/);
  });

  it.each([
    [[], /rule 'colons'/],
    [5, /rule 'colons'/],
    [{ enabled: 'yes' }, /rule 'colons', option 'enabled'/],
    [{ 'max-spaces-after': 'a' }, /rule 'colons', option 'max-spaces-after'/],
  ])('bad entry %j for a rule names the rule key', (entry, pattern) => {
    expect(() => lint('a: 1\n', bad({ colons: entry }))).toThrow(pattern);
  });

  it('unknown disabled rule', () => {
    expect(() => lint('a: 1\n', { disabledRules: ['no-such-rule'] })).toThrow(/no-such-rule/);
  });

  it.each([0, -1, 2 ** 32 + 2, 2 ** 32, 1.5, Number.NaN, Number.POSITIVE_INFINITY])(
    'maxLineLength %s is rejected',
    (value) => {
      expect(() => lint('a: 1\n', { maxLineLength: value })).toThrow(
        /maxLineLength must be a positive integer/
      );
    }
  );

  it.each([0, 99, -1, 2 ** 32 + 2, 1.5, Number.NaN, Number.NEGATIVE_INFINITY])(
    'indentSize %s is rejected',
    (value) => {
      expect(() => lint('a: 1\n', { indentSize: value })).toThrow(
        /indentSize must be an integer between 1 and 16/
      );
    }
  );

  it('accepts boundary values', () => {
    expect(() => lint('a: 1\n', { maxLineLength: 4294967295, indentSize: 16 })).not.toThrow();
    expect(() => lint('a: 1\n', { maxLineLength: 1, indentSize: 1 })).not.toThrow();
  });

  it('config error leaves later calls unaffected', () => {
    expect(() => lint('a: 1\n', bad({ 'no-such-rule': 'error' }))).toThrow();
    expect(() => lint('a: 1\n')).not.toThrow();
  });
});

describe('yamllint forms and shorthands', () => {
  it('quoted-strings required: false means not required', () => {
    const source = 'a: "x"\n';
    expect(codes(source)).toContain('quoted-strings');
    expect(codes(source, { rules: { 'quoted-strings': { required: false } } })).not.toContain(
      'quoted-strings'
    );
  });

  it('braces forbid: true forbids flow mappings', () => {
    expect(codes('a: {b: 1}\n', { rules: { braces: { forbid: true } } })).toContain('braces');
    expect(codes('a: {b: 1}\n')).not.toContain('braces');
  });

  it('disable and enable shorthands', () => {
    const source = 'b: 1\na: 2\n';
    expect(codes(source, { rules: { 'key-ordering': 'enable' } })).toContain('key-ordering');
    expect(codes(source, { rules: { 'key-ordering': 'disable' } })).not.toContain('key-ordering');
  });

  it('unquoted booleans in truthy allowed-values are rejected with a quoting hint', () => {
    expect(() => lint('a: 1\n', bad({ truthy: { 'allowed-values': [true, false] } }))).toThrow(
      /rule 'truthy', option 'allowed-values\[0\]'.*ambiguous.*quote/
    );
  });

  it('document-end present: false forbids the marker', () => {
    const rules = { rules: { 'document-end': { present: false } } } as LintConfig;
    const messages = lint('a: 1\n...\n', rules)
      .filter((d) => d.code === 'document-end')
      .map((d) => d.message);
    expect(messages).toEqual(["document end marker '...' is forbidden"]);
    expect(codes('a: 1\n', rules)).not.toContain('document-end');
  });

  it('quoted-strings extra-required regex flags matching plain scalars', () => {
    const config = {
      rules: { 'quoted-strings': { 'extra-required': ['^http://', '\\.md$'] } },
    } as LintConfig;
    const messages = lint('a: http://x\nb: README.md\nc: plain\n', config)
      .filter((d) => d.code === 'quoted-strings')
      .map((d) => d.message);
    expect(messages).toEqual(['string should be quoted', 'string should be quoted']);
  });

  it('quoted-strings extra-allowed regex keeps plain scalars', () => {
    const config = {
      rules: { 'quoted-strings': { 'extra-allowed': ['^ftp://'] } },
    } as LintConfig;
    const messages = lint('a: ftp://x\nb: "ftp://x"\nc: "plain"\n', config)
      .filter((d) => d.code === 'quoted-strings')
      .map((d) => d.message);
    expect(messages).toEqual(['string does not need quotes']);
  });

  it('invalid regex names rule, option and index', () => {
    expect(() =>
      lint('a: 1\n', bad({ 'quoted-strings': { 'extra-required': ['ok', '(?=x)'] } }))
    ).toThrow(/rule 'quoted-strings', option 'extra-required'.*pattern 1.*look-around/);
  });

  it('extra-allowed with required: always is rejected', () => {
    expect(() =>
      lint('a: 1\n', bad({ 'quoted-strings': { required: 'always', 'extra-allowed': ['a'] } }))
    ).toThrow(/extra-allowed.*only-when-needed/);
  });

  it('check-multi-line-strings is supported and a typo is rejected', () => {
    expect(() =>
      lint('a: 1\n', bad({ indentation: { 'check-multi-line-strings': true } }))
    ).not.toThrow();
    expect(() => lint('a: 1\n', bad({ indentation: { 'check-multi-line-string': true } }))).toThrow(
      /check-multi-line-string/
    );
  });
});

describe('severity strings', () => {
  it.each(['error', 'ERROR', 'Warning', 'INFO', 'hint'])('accepts %s', (severity) => {
    const result = lint('a: 1\na: 2\n', bad({ 'duplicate-key': severity }));
    expect(result.find((d) => d.code === 'duplicate-key')?.severity.toLowerCase()).toBe(
      severity.toLowerCase()
    );
  });

  it('rejects an unknown severity', () => {
    expect(() => lint('a: 1\n', bad({ 'duplicate-key': 'critical' }))).toThrow(/critical/);
  });
});

const fy =
  process.env.FY_BIN ??
  resolve(__dirname, `../../target/debug/fy${process.platform === 'win32' ? '.exe' : ''}`);
const workDir = mkdtempSync(join(tmpdir(), 'fy-node-'));
afterAll(() => rmSync(workDir, { recursive: true, force: true }));

interface CliDiagnostic {
  code: string;
  severity: string;
  message: string;
  span: { start: { line: number; column: number } };
}

const summarize = (
  diags: { code: string; severity: string; message: string; line: number; column: number }[]
) =>
  diags
    .map((d) => `${d.line}:${d.column} ${d.code} ${d.severity.toLowerCase()} ${d.message}`)
    .sort();

const fyMissing = !existsSync(fy);
const inCi = Boolean(process.env.CI);

it.runIf(inCi)('fy binary is available in CI', () => {
  expect(fyMissing, `fy binary not found at ${fy}; set FY_BIN`).toBe(false);
});

describe.skipIf(fyMissing && !inCi)('parity with fy lint --config', () => {
  const cases: [string, string, string, LintConfig][] = [
    [
      'quoted-strings and document-start',
      'a: "x"\nb: y\n',
      'rules:\n  document-start: {present: true}\n  quoted-strings: {quote-type: single, required: true}\n',
      {
        rules: {
          'document-start': { present: true },
          'quoted-strings': { 'quote-type': 'single', required: true },
        },
      },
    ],
    [
      'line-length max and severity shorthand',
      'key: this line is longer than twenty\nkey2: 1\nkey2: 2\n',
      'rules:\n  line-length: {max: 20, severity: error}\n  duplicate-key: info\n',
      { rules: { 'line-length': { max: 20, severity: 'error' }, 'duplicate-key': 'info' } },
    ],
  ];

  it.each(cases)('%s', (name, source, cliConfig, nodeConfig) => {
    const slug = name.replace(/\W+/g, '-');
    const input = join(workDir, `${slug}.yaml`);
    const config = join(workDir, `${slug}.config.yaml`);
    writeFileSync(input, source);
    writeFileSync(config, cliConfig);
    const out = spawnSync(
      fy,
      ['lint', '--no-color', '--format', 'json', '--config', config, input],
      { encoding: 'utf8' }
    );
    const cli = (JSON.parse(out.stdout) as CliDiagnostic[]).map((d) => ({
      code: d.code,
      severity: d.severity,
      message: d.message,
      line: d.span.start.line,
      column: d.span.start.column,
    }));
    const node = lint(source, nodeConfig).map((d) => ({
      code: d.code,
      severity: d.severity,
      message: d.message,
      line: d.span.start.line,
      column: d.span.start.column,
    }));
    expect(node.length).toBeGreaterThan(0);
    expect(summarize(node)).toEqual(summarize(cli));
  });

  it('reports the same error text as the CLI', () => {
    const config = join(workDir, 'typo.config.yaml');
    writeFileSync(config, 'rules:\n  quoted-strings: {quote-type: singel}\n');
    const out = spawnSync(fy, ['lint', '--no-color', '--config', config, config], {
      encoding: 'utf8',
    });
    let message = '';
    try {
      lint('a: 1\n', { rules: { 'quoted-strings': { 'quote-type': 'singel' } } as never });
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).not.toBe('');
    expect(out.stderr).toContain(message);
  });
});

describe('yamllint parity options (#536)', () => {
  const codesOf = (source: string, rules: Record<string, unknown>) =>
    lint(source, bad(rules)).map((d) => d.code);

  it('level is an alias of severity', () => {
    const result = lint(
      `key: ${'x'.repeat(20)}\n`,
      bad({ 'line-length': { max: 10, level: 'warning' } })
    );
    expect(result.find((d) => d.code === 'line-length')?.severity).toBe('Warning');
    expect(() =>
      lint('a: 1\n', bad({ 'line-length': { level: 'warning', severity: 'error' } }))
    ).toThrow(/aliases/);
  });

  it('truthy does not report single letters', () => {
    const rules = { truthy: { 'check-keys': true } };
    expect(codesOf('a: y\nb: N\ny: 1\n', rules)).not.toContain('truthy');
    expect(codesOf('a: yes\n', rules)).toContain('truthy');
  });

  it('quoted-strings check-keys and allow-quoted-quotes', () => {
    expect(codesOf('"a": "b"\n', { 'quoted-strings': { required: true } })).not.toContain(
      'quoted-strings'
    );
    expect(
      codesOf('a: "b"\n', { 'quoted-strings': { required: true, 'check-keys': true } })
    ).toContain('quoted-strings');
    const quotes = {
      'quoted-strings': { 'quote-type': 'single', required: false, 'allow-quoted-quotes': true },
    };
    expect(codesOf('a: "it\'s"\n', quotes)).not.toContain('quoted-strings');
    expect(codesOf('a: "plain"\n', quotes)).toContain('quoted-strings');
  });

  it('forbid-duplicated-merge-keys', () => {
    const source = 'a: &a {x: 1}\nb: &b {y: 2}\nc:\n  <<: *a\n  <<: *b\n';
    expect(codesOf(source, {})).toContain('duplicate-key');
    expect(
      codesOf(source, { 'duplicate-key': { 'forbid-duplicated-merge-keys': false } })
    ).not.toContain('duplicate-key');
  });

  it('line-length non-breakable options', () => {
    const url = 'http://localhost/very/very/very/very/very/very/very/very/long/url';
    expect(codesOf(`- ${url}\n`, { 'line-length': { max: 20 } })).not.toContain('line-length');
    expect(
      codesOf(`- ${url}\n`, { 'line-length': { max: 20, 'allow-non-breakable-words': false } })
    ).toContain('line-length');
    expect(
      codesOf(`key: ${url}\n`, {
        'line-length': { max: 20, 'allow-non-breakable-inline-mappings': true },
      })
    ).not.toContain('line-length');
  });

  it('document markers are checked per document', () => {
    const start = { 'document-start': { present: true } };
    expect(codesOf('---\na: 1\n...\n---\nb: 2\n', start)).not.toContain('document-start');
    const lines = lint('a: 1\n---\nb: 2\n', bad(start))
      .filter((d) => d.code === 'document-start')
      .map((d) => d.span.start.line);
    expect(lines).toEqual([1]);
  });
});
