/* hand-written; napi injects it into index.d.ts (package.json napi.dtsHeaderFile) */
/* eslint-disable */
/** Diagnostic severity as accepted in rule configuration (case-insensitive). */
export type RuleSeverity = 'error' | 'warning' | 'info' | 'hint' | 'Error' | 'Warning' | 'Info' | 'Hint'

/** Keys common to every rule entry. */
export interface RuleEntryBase {
  /** Whether the rule runs (default: true). */
  enabled?: boolean
  /** Severity override for the rule. */
  severity?: RuleSeverity
}

/** Spacing options shared by `braces` and `brackets`. */
export interface FlowRuleOptions {
  forbid?: boolean | 'non-empty' | 'all'
  'min-spaces-inside'?: number
  'max-spaces-inside'?: number
  'min-spaces-inside-empty'?: number
  'max-spaces-inside-empty'?: number
}

/** Options of each built-in rule, keyed by rule code. */
export interface RuleOptionsByRule {
  braces: FlowRuleOptions
  brackets: FlowRuleOptions
  colons: { 'max-spaces-before'?: number; 'max-spaces-after'?: number }
  commas: { 'max-spaces-before'?: number; 'min-spaces-after'?: number; 'max-spaces-after'?: number }
  hyphens: { 'max-spaces-after'?: number }
  comments: {
    'require-starting-space'?: boolean
    'ignore-shebangs'?: boolean
    'min-spaces-from-content'?: number
  }
  'comments-indentation': {}
  'document-start': { present?: boolean | 'required' | 'forbidden' | 'allowed' }
  'document-end': { present?: true | 'required' | 'allowed' }
  'empty-lines': { max?: number; 'max-start'?: number; 'max-end'?: number }
  'empty-values': {
    'forbid-in-block-mappings'?: boolean
    'forbid-in-flow-mappings'?: boolean
    'forbid-in-block-sequences'?: boolean
  }
  'float-values': {
    'require-numeral-before-decimal'?: boolean
    'forbid-scientific-notation'?: boolean
    'forbid-nan'?: boolean
    'forbid-inf'?: boolean
  }
  indentation: { 'indent-size'?: number }
  'key-ordering': { 'case-sensitive'?: boolean }
  'line-length': { max?: number | null }
  'new-lines': { type?: 'unix' | 'dos' | 'platform' }
  'new-line-at-end-of-file': {}
  'octal-values': { 'forbid-implicit-octal'?: boolean; 'forbid-explicit-octal'?: boolean }
  'quoted-strings': {
    'quote-type'?: 'any' | 'single' | 'double'
    required?: boolean | 'always' | 'not-required' | 'only-when-needed' | 'never'
    'extra-required'?: string[]
    'extra-allowed'?: string[]
  }
  'trailing-whitespace': {}
  truthy: { 'allowed-values'?: string[]; 'check-keys'?: boolean }
  'duplicate-key': {}
  'invalid-anchor': {}
}

/** Code of a built-in lint rule. */
export type LintRuleName = keyof RuleOptionsByRule

/** One rule entry: a severity, `'enable'`, `'disable'` or an object with options. */
export type RuleEntry<R extends LintRuleName> =
  | RuleSeverity
  | 'enable'
  | 'disable'
  | null
  | (RuleEntryBase & RuleOptionsByRule[R])

/** Per-rule configuration patch; the same shape as `rules:` in the `fy lint --config` file. */
export type LintRulesConfig = { [R in LintRuleName]?: RuleEntry<R> }
