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
  /** yamllint spelling of `severity`; cannot be combined with it. */
  level?: RuleSeverity
  /** Gitignore-style patterns of files the rule skips, relative to the working directory. */
  ignore?: string | string[]
  /** Files with one ignore pattern per line, relative to the working directory; excludes `ignore`. */
  'ignore-from-file'?: string | string[]
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
  'document-end': { present?: boolean | 'required' | 'forbidden' | 'allowed' }
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
  'line-length': {
    max?: number | null
    'allow-non-breakable-words'?: boolean
    'allow-non-breakable-inline-mappings'?: boolean
  }
  'new-lines': { type?: 'unix' | 'dos' | 'platform' }
  'new-line-at-end-of-file': {}
  'set-values': {}
  'octal-values': { 'forbid-implicit-octal'?: boolean; 'forbid-explicit-octal'?: boolean }
  'quoted-strings': {
    'quote-type'?: 'any' | 'single' | 'double'
    required?: boolean | 'always' | 'not-required' | 'only-when-needed' | 'never'
    'extra-required'?: string[]
    'extra-allowed'?: string[]
    'allow-quoted-quotes'?: boolean
    'check-keys'?: boolean
  }
  'trailing-whitespace': {}
  truthy: { 'allowed-values'?: string[]; 'check-keys'?: boolean }
  'duplicate-key': { 'forbid-duplicated-merge-keys'?: boolean }
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
export type LintRulesConfig = { [R in LintRuleName]?: RuleEntry<R> } & {
  /** yamllint's name for `duplicate-key`. */
  'key-duplicates'?: RuleEntry<'duplicate-key'>
  /** yamllint's name for `trailing-whitespace`. */
  'trailing-spaces'?: RuleEntry<'trailing-whitespace'>
  /** yamllint's name for `invalid-anchor`. */
  anchors?: RuleEntry<'invalid-anchor'>
}
/**
 * YAML linter with configurable rules.
 *
 * # Example
 *
 * ```javascript
 * const { Linter } = require('@fast-yaml/core');
 * const linter = Linter.withAllRules();
 * const diagnostics = linter.lint('name: value
name: duplicate');
 * ```
 */
export declare class Linter {
  /**
   * Creates a new linter with optional configuration.
   *
   * # Errors
   *
   * Returns an error if the configuration is invalid.
   */
  constructor(config?: LintConfig | undefined | null)
  /** Creates a linter with all default rules enabled. */
  static withAllRules(): Linter
  /**
   * Lints YAML source code and returns diagnostics.
   *
   * `path` is the file the source comes from; rules whose `ignore` patterns match it are
   * skipped. Omit it for standard input.
   *
   * # Errors
   *
   * Returns an error if the YAML cannot be parsed, the source exceeds `maxInputBytes`
   * (default 100 MiB), or the directory of `path` does not exist.
   */
  lint(source: string, path?: string | undefined | null): Array<Diagnostic>
}

/**
 * Represents a position in a YAML source file.
 *
 * Used to indicate where errors occur during parsing.
 *
 * # Example
 *
 * ```javascript
 * const { Mark } = require('@fast-yaml/core');
 *
 * const mark = new Mark('<input>', 5, 10);
 * console.log(mark.name);   // '<input>'
 * console.log(mark.line);   // 5
 * console.log(mark.column); // 10
 * console.log(mark.toString()); // '<input>:5:10'
 * ```
 */
export declare class Mark {
  /** The name of the source (e.g., filename or '<input>'). */
  readonly name: string
  /** The line number (0-indexed). */
  readonly line: number
  /** The column number (0-indexed). */
  readonly column: number
  /**
   * Create a new Mark instance.
   *
   * # Arguments
   *
   * * `name` - The source name (e.g., filename)
   * * `line` - The line number (0-indexed)
   * * `column` - The column number (0-indexed)
   *
   * # Errors
   *
   * Returns an `InvalidArg` error if `line` or `column` is not an integer in `0..=4294967295`.
   */
  constructor(name: string, line: number, column: number)
  /**
   * Get a string representation of the mark.
   *
   * Returns format: "name:line:column"
   */
  toString(): string
}

/** Configuration for batch file processing. */
export interface BatchConfig {
  /** Worker count (null = auto, 0 = sequential) */
  workers?: number
  /** Maximum input size in bytes per file (integer, 1..=1073741824, default: 104857600) */
  maxInputBytes?: number
  /** Removed: renamed to `maxInputBytes`; passing it throws. */
  maxInputSize?: number
  /** Sequential threshold (default: 4KB) */
  sequentialThreshold?: number
  /** Indentation width in spaces (integer, 1..=9, default: 2) */
  indent?: number
  /** Maximum line width (integer, 20..=1000, default: 80) */
  width?: number
  /** Sort dictionary keys alphabetically (default: false) */
  sortKeys?: boolean
  /**
   * Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections stop at 255 levels;
   * applies to `processFiles` and `formatFiles`.
   * Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. `formatFiles` rejects input nested deeper than this limit.
   */
  maxDepth?: number
  /**
   * Maximum estimated alias-expansion bytes per file (integer, 1..=1073741824,
   * default: 67108864); applies to `processFiles` only; peak memory can reach workers x this budget
   */
  maxAliasBytes?: number
  /**
   * Maximum characters the parser may read past the last node it reported (integer,
   * 1..=1073741824, default: 4194304); applies to `processFiles` and `formatFiles`. A flow
   * collection at the root or in a `- ` entry, one scalar, or a run of comments longer than
   * this is rejected; parser memory is bounded by about 190 times this value.
   */
  maxScanAhead?: number
  /**
   * Maximum number of documents per file (integer, 1..=10000000, default: 100000); applies to
   * `processFiles` and `formatFiles`.
   */
  maxDocuments?: number
}

/** Error entry for batch result. */
export interface BatchError {
  /** Path to the failed file */
  path: string
  /** Error message */
  message: string
}

/** Aggregated results from batch processing. */
export interface BatchResult {
  /** Total number of files processed */
  total: number
  /** Number of files successfully processed */
  success: number
  /** Number of files changed */
  changed: number
  /** Number of files that failed */
  failed: number
  /** Total processing duration in milliseconds */
  durationMs: number
  /** List of errors with file paths */
  errors: Array<BatchError>
}

/** A single line of source context. */
export interface ContextLine {
  /** Line number (1-indexed). */
  lineNumber: number
  /** Source text content. */
  content: string
  /** Number of chars of the line dropped before `content`. */
  columnOffset: number
  /** Whether chars of the line were dropped after `content`. */
  truncatedEnd: boolean
  /** Highlight ranges as [[start, end], ...] (absolute column positions). */
  highlights: Array<Array<number>>
}

/** A diagnostic message with location and context. */
export interface Diagnostic {
  /** Diagnostic code (e.g., "duplicate-key"). */
  code: string
  /** Severity level. */
  severity: Severity
  /** Primary error message. */
  message: string
  /** Location span where the error occurred. */
  span: Span
  /** Additional context for display. */
  context?: DiagnosticContext
  /** Suggested fixes. */
  suggestions: Array<Suggestion>
}

/** Source code context for diagnostics. */
export interface DiagnosticContext {
  /** Source lines surrounding the diagnostic. */
  lines: Array<ContextLine>
}

/** Options for YAML serialization. */
export interface DumpOptions {
  /** If true, sort object keys alphabetically (default: false) */
  sortKeys?: boolean
  /**
   * Allow unicode characters (default: true).
   * Note: yaml-rust2 always outputs unicode; this is accepted for API compatibility.
   */
  allowUnicode?: boolean
  /**
   * Indentation width in spaces (default: 2).
   * Must be an integer in 1-9; other values throw.
   */
  indent?: number
  /**
   * Maximum line width for wrapping (default: 80).
   * Must be an integer in 20-1000; other values throw.
   */
  width?: number
  /**
   * Default flow style for collections (default: null).
   * - null: Use block style (multi-line)
   * - true: Force flow style (inline: [...], {...})
   * - false: Force block style (explicit)
   */
  defaultFlowStyle?: boolean
  /** Add explicit document start marker `---` (default: false). */
  explicitStart?: boolean
}

/** Outcome of processing a single file. */
export declare const enum FileOutcome {
  /** File processed successfully */
  Success = 'Success',
  /** File formatted and content changed */
  Changed = 'Changed',
  /** File unchanged (already formatted) */
  Unchanged = 'Unchanged',
  /** Processing failed */
  Error = 'Error'
}

/** Result for a single file with path context. */
export interface FileResult {
  /** Path to the processed file */
  path: string
  /** Processing outcome */
  outcome: FileOutcome
  /** Processing duration in milliseconds */
  durationMs: number
  /** Error message if outcome is Error */
  error?: string
}

/**
 * Format files and return formatted content (dry-run).
 *
 * Formats YAML files without writing changes back.
 *
 * # Arguments
 *
 * * `paths` - Array of file paths to format
 * * `config` - Optional batch processing configuration
 *
 * # Returns
 *
 * Array of `FormatResult` objects
 *
 * # Example
 *
 * ```javascript
 * const { formatFiles } = require('fastyaml-rs');
 * const results = formatFiles(['file1.yaml']);
 * results.forEach(r => {
 *   if (r.content) console.log(r.content);
 * });
 * ```
 */
export declare function formatFiles(paths: Array<string>, config?: BatchConfig | undefined | null): Array<FormatResult>

/**
 * Format files in place (write changes back).
 *
 * Formats YAML files and writes changes atomically.
 * Only modified files are written.
 *
 * # Arguments
 *
 * * `paths` - Array of file paths to format
 * * `config` - Optional batch processing configuration
 *
 * # Returns
 *
 * `BatchResult` with changed/unchanged counts
 *
 * # Example
 *
 * ```javascript
 * const { formatFilesInPlace } = require('fastyaml-rs');
 * const result = formatFilesInPlace(['file1.yaml', 'file2.yaml']);
 * console.log(`Changed ${result.changed} files`);
 * ```
 */
export declare function formatFilesInPlace(paths: Array<string>, config?: BatchConfig | undefined | null): BatchResult

/** Formatted file result. */
export interface FormatResult {
  /** Path to the file */
  path: string
  /** Formatted content (null if error) */
  content?: string
  /** Error message (null if success) */
  error?: string
}

/**
 * Lint YAML source with optional configuration.
 *
 * Convenience function equivalent to `Linter.withAllRules().lint(source, path)`.
 *
 * # Errors
 *
 * Returns an error if the YAML cannot be parsed, the source exceeds `maxInputBytes`
 * (default 100 MiB), or the directory of `path` does not exist.
 *
 * # Example
 *
 * ```javascript
 * const { lint } = require('@fast-yaml/core');
 * const diagnostics = lint('key: value
key: duplicate');
 * ```
 */
export declare function lint(source: string, config?: LintConfig | undefined | null, path?: string | undefined | null): Array<Diagnostic>

/**
 * Configuration for the linter.
 *
 * All fields are optional; defaults are applied during conversion.
 */
export interface LintConfig {
  /** Maximum line length; unset keeps the rule default, `0` is an error. */
  maxLineLength?: number
  /** Expected indentation size in spaces. */
  indentSize?: number
  /** Require document start marker (---). */
  requireDocumentStart?: boolean
  /** Require document end marker (...). */
  requireDocumentEnd?: boolean
  /** Allow duplicate keys (non-compliant). */
  allowDuplicateKeys?: boolean
  /** Disabled rule codes. */
  disabledRules?: Array<string>
  /**
   * Per-rule configuration patch, applied after the fields above.
   *
   * Each key is a rule code; the value is a severity string (case-insensitive),
   * or an object with `enabled`, `severity` and the rule's own options
   * (kebab-case keys, as in the `fy lint --config` file). Unknown rules, unknown
   * options, wrong types and `null` (except `line-length.max`) are errors.
   * `disabledRules` is applied last and wins over `enabled: true`. The value is read
   * under depth and size limits, so deeply nested or cyclic input is an `InvalidArg` error.
   */
  rules?: LintRulesConfig
  /**
   * Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections (`[]`, `{}`) stop at 255 levels whatever this is.
   * Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
   */
  maxDepth?: number
  /** Maximum estimated alias-expansion bytes (integer, 1..=1073741824, default: 67108864). */
  maxAliasBytes?: number
  /**
   * Maximum characters the parser may read past the last node it reported (integer,
   * 1..=1073741824, default: 4194304). A flow collection at the root or in a `- ` entry, one
   * scalar, or a run of comments longer than this is rejected; parser memory is bounded by
   * about 190 times this value.
   */
  maxScanAhead?: number
  /**
   * Largest source accepted for linting, in bytes (integer, 1..=1073741824, default: 104857600).
   * Bounds linting work on oversized input; the source is already in memory when checked, so this is not a memory bound.
   */
  maxInputBytes?: number
  /** Maximum number of documents in the stream (integer, 1..=10000000, default: 100000). */
  maxDocuments?: number
}

/**
 * Parse a YAML string with options (js-yaml compatible).
 *
 * This is the js-yaml compatible `load()` function that accepts an options object.
 * Currently all schemas behave as `SafeSchema` (safe by default).
 *
 * # Arguments
 *
 * * `yaml_str` - A YAML document as a string
 * * `options` - Optional parsing options (schema, filename, etc.)
 *
 * # Returns
 *
 * The parsed YAML document as JavaScript objects
 *
 * # Errors
 *
 * Throws an error if:
 * - The YAML is invalid
 * - Input exceeds size limit (100MB)
 *
 * # Example
 *
 * ```javascript
 * const { load, SAFE_SCHEMA } = require('@fast-yaml/core');
 *
 * const data = load('name: test', { schema: 'SafeSchema' });
 * console.log(data); // { name: 'test' }
 * ```
 */
export declare function load(yamlStr: string, options?: LoadOptions | undefined | null): unknown

/**
 * Parse a YAML string containing multiple documents with options (js-yaml compatible).
 *
 * This is the js-yaml compatible `loadAll()` function that accepts an options object.
 * Currently all schemas behave as `SafeSchema` (safe by default).
 *
 * # Arguments
 *
 * * `yaml_str` - A YAML string potentially containing multiple documents
 * * `options` - Optional parsing options (schema, filename, etc.)
 *
 * # Returns
 *
 * An array of parsed JavaScript objects
 *
 * # Errors
 *
 * Throws an error if:
 * - The YAML is invalid
 * - Input exceeds size limit (100MB)
 *
 * # Example
 *
 * ```javascript
 * const { loadAll, SAFE_SCHEMA } = require('@fast-yaml/core');
 *
 * const docs = loadAll('---
foo: 1
---
bar: 2', { schema: 'SafeSchema' });
 * console.log(docs); // [{ foo: 1 }, { bar: 2 }]
 * ```
 */
export declare function loadAll(yamlStr: string, options?: LoadOptions | undefined | null): Array<unknown>

/** Options for YAML parsing (js-yaml compatible). */
export interface LoadOptions {
  /**
   * YAML schema to use for parsing (default: `SafeSchema`).
   * Currently all schemas behave as `SafeSchema` (safe by default).
   */
  schema?: Schema
  /** Filename or source name for error messages (default: `<input>`). */
  filename?: string
  /**
   * Allow duplicate keys in mappings (default: true).
   * Note: fast-yaml always allows duplicates; this is for API compatibility.
   */
  allowDuplicateKeys?: boolean
  /**
   * Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections (`[]`, `{}`) stop at 255 levels whatever this is.
   * Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
   */
  maxDepth?: number
  /**
   * Maximum estimated bytes produced by alias expansion per call (integer,
   * 1..=1073741824, default: 67108864). Host objects cost several times the estimate.
   */
  maxAliasBytes?: number
  /**
   * Maximum characters the parser may read past the last node it reported (integer,
   * 1..=1073741824, default: 4194304). A flow collection at the root or in a `- ` entry, one
   * scalar, or a run of comments longer than this is rejected; parser memory is bounded by
   * about 190 times this value.
   */
  maxScanAhead?: number
  /** Maximum number of documents in the stream (integer, 1..=10000000, default: 100000). */
  maxDocuments?: number
}

/** A position in the source file. */
export interface Location {
  /** Line number (1-indexed). */
  line: number
  /** Column number (1-indexed). */
  column: number
  /** Byte offset in the text with document-prefix BOMs removed (0-indexed), also for suggestion spans. */
  offset: number
}

/**
 * Configuration for parallel YAML processing.
 *
 * Controls thread pool size, chunking thresholds, and resource limits.
 *
 * # Example
 *
 * ```javascript
 * const { parseParallel, ParallelConfig } = require('fastyaml-rs');
 *
 * const config = {
 *   threadCount: 8,
 *   maxInputBytes: 200 * 1024 * 1024
 * };
 * const docs = parseParallel(yamlString, config);
 * ```
 */
export interface ParallelConfig {
  /** Thread pool size (null = CPU count, 0 = sequential). */
  threadCount?: number
  /** Minimum bytes per chunk (default: 4096). */
  minChunkSize?: number
  /** Maximum total input size in bytes (integer, 1..=1073741824, default: 104857600). */
  maxInputBytes?: number
  /** Removed: renamed to `maxInputBytes`; passing it throws. */
  maxInputSize?: number
  /** Maximum number of documents allowed (integer, 1..=10000000, default: 100000). */
  maxDocuments?: number
  /**
   * Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections (`[]`, `{}`) stop at 255 levels whatever this is.
   * Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
   */
  maxDepth?: number
  /**
   * Maximum estimated alias-expansion bytes, shared across chunks of one call (integer,
   * 1..=1073741824, default: 67108864).
   */
  maxAliasBytes?: number
  /**
   * Maximum characters the parser may read past the last node it reported (integer,
   * 1..=1073741824, default: 4194304). A flow collection at the root or in a `- ` entry, one
   * scalar, or a run of comments longer than this is rejected; parser memory is bounded by
   * about 190 times this value.
   */
  maxScanAhead?: number
}

/**
 * Parse multi-document YAML in parallel (synchronous).
 *
 * Automatically splits YAML documents at '---' boundaries and
 * processes them in parallel using Rayon thread pool.
 *
 * # Arguments
 *
 * * `yaml_str` - YAML source potentially containing multiple documents
 * * `config` - Optional parallel processing configuration
 *
 * # Returns
 *
 * Array of parsed YAML documents
 *
 * # Errors
 *
 * Throws if parsing fails or limits exceeded
 *
 * # Performance
 *
 * - Single document: Falls back to sequential parsing
 * - Multi-document: 2-3x faster on 4-8 core systems
 * - Use for files > 1MB with multiple documents
 *
 * # Example
 *
 * ```javascript
 * const { parseParallel } = require('fastyaml-rs');
 *
 * const yaml = '---
foo: 1
---
bar: 2
---
baz: 3';
 * const docs = parseParallel(yaml);
 * console.log(docs.length); // 3
 * ```
 */
export declare function parseParallel(yamlStr: string, config?: ParallelConfig | undefined | null): Array<unknown>

/**
 * Parse multi-document YAML in parallel (asynchronous).
 *
 * Non-blocking version that runs parsing on Node.js worker thread pool.
 * Useful for keeping the event loop responsive during large file parsing.
 *
 * # Arguments
 *
 * * `yaml_str` - YAML source potentially containing multiple documents
 * * `config` - Optional parallel processing configuration
 *
 * # Returns
 *
 * Promise resolving to array of parsed YAML documents
 *
 * # Example
 *
 * ```javascript
 * const { parseParallelAsync } = require('fastyaml-rs');
 *
 * const yaml = '---
foo: 1
---
bar: 2';
 * const docs = await parseParallelAsync(yaml);
 * console.log(docs); // [{ foo: 1 }, { bar: 2 }]
 * ```
 */
export declare function parseParallelAsync(yamlStr: string, config?: ParallelConfig | undefined | null): Promise<unknown>

/**
 * Process files and return batch result.
 *
 * Parses and validates YAML files in parallel.
 *
 * # Arguments
 *
 * * `paths` - Array of file paths to process
 * * `config` - Optional batch processing configuration
 *
 * # Returns
 *
 * `BatchResult` with processing statistics
 *
 * # Example
 *
 * ```javascript
 * const { processFiles } = require('fastyaml-rs');
 * const result = processFiles(['file1.yaml', 'file2.yaml']);
 * console.log(`Processed ${result.total} files, ${result.failed} failed`);
 * ```
 */
export declare function processFiles(paths: Array<string>, config?: BatchConfig | undefined | null): BatchResult

/**
 * Serialize a JavaScript object to a YAML string.
 *
 * This is equivalent to js-yaml's `safeDump()` and `PyYAML`'s `safe_dump()`.
 *
 * # Arguments
 *
 * * `data` - A JavaScript object to serialize (Object, Array, Set, Map, string, number, boolean, null)
 * * `options` - Optional serialization options
 *
 * # Returns
 *
 * A YAML string representation of the object
 *
 * # Sets and maps
 *
 * A `Set` is written as a `!!set` and a `Map` as a mapping. `safeLoad` reads a `!!set` back as an
 * object with `null` values (js-yaml's form), so the mapping is one-way.
 *
 * # Errors
 *
 * Throws an error if the object contains non-serializable types, or if two `Set` members or
 * `Map` keys are the same YAML value.
 *
 * # Example
 *
 * ```javascript
 * const { safeDump } = require('@fast-yaml/core');
 *
 * const yaml = safeDump({ name: 'test', value: 123 });
 * console.log(yaml); // 'name: test
value: 123
'
 * ```
 */
export declare function safeDump(data: unknown, options?: DumpOptions | undefined | null): string

/**
 * Serialize multiple JavaScript objects to a YAML string with document separators.
 *
 * This is equivalent to js-yaml's `safeDumpAll()` and `PyYAML`'s `safe_dump_all()`.
 *
 * # Arguments
 *
 * * `documents` - An array of JavaScript objects to serialize
 * * `options` - Optional serialization options
 *
 * # Returns
 *
 * A YAML string with multiple documents separated by "---"
 *
 * # Errors
 *
 * Throws an error if:
 * - Any object cannot be serialized
 * - Total output size exceeds 100MB limit
 *
 * # Security
 *
 * Maximum output size is limited to 100MB to prevent memory exhaustion.
 *
 * # Example
 *
 * ```javascript
 * const { safeDumpAll } = require('@fast-yaml/core');
 *
 * const yaml = safeDumpAll([{ a: 1 }, { b: 2 }]);
 * console.log(yaml); // '---
a: 1
---
b: 2
'
 * ```
 */
export declare function safeDumpAll(documents: Array<unknown>, options?: DumpOptions | undefined | null): string

/**
 * Parse a YAML string and return a JavaScript object.
 *
 * This is equivalent to js-yaml's `safeLoad()` and `PyYAML`'s `safe_load()`.
 *
 * # Arguments
 *
 * * `yaml_str` - A YAML document as a string
 * * `options` - Optional parsing options; `maxDepth`, `maxAliasBytes`, `maxScanAhead` and `maxDocuments` raise or lower the resource limits
 *
 * # Returns
 *
 * The parsed YAML document as JavaScript objects (Object, Array, string, number, boolean, null).
 * A `!!set` loads as an object whose members are keys with `null` values, like js-yaml.
 *
 * # Errors
 *
 * Throws an error if:
 * - The YAML is invalid
 * - A `!!set` member has a non-null value, a `<<` key is repeated in one mapping, or two keys differ in
 *   YAML but share a JavaScript property name (`1` and `"1"`); the message carries the position
 * - Input exceeds size limit (100MB)
 *
 * # Security
 *
 * Maximum input size is limited to 100MB to prevent denial-of-service attacks.
 *
 * # Example
 *
 * ```javascript
 * const { safeLoad } = require('@fast-yaml/core');
 *
 * const data = safeLoad('name: test
value: 123');
 * console.log(data); // { name: 'test', value: 123 }
 * ```
 */
export declare function safeLoad(yamlStr: string, options?: LoadOptions | undefined | null): unknown

/**
 * Parse a YAML string containing multiple documents.
 *
 * This is equivalent to js-yaml's `safeLoadAll()` and `PyYAML`'s `safe_load_all()`.
 *
 * # Arguments
 *
 * * `yaml_str` - A YAML string potentially containing multiple documents
 * * `options` - Optional parsing options; `maxDepth`, `maxAliasBytes`, `maxScanAhead` and `maxDocuments` raise or lower the resource limits
 *
 * # Returns
 *
 * An array of parsed JavaScript objects
 *
 * # Errors
 *
 * Throws an error if:
 * - The YAML is invalid
 * - Input exceeds size limit (100MB)
 * - `maxDepth`, `maxAliasBytes`, `maxScanAhead` or `maxDocuments` is not an integer within its range
 *
 * # Security
 *
 * Maximum input size is limited to 100MB to prevent denial-of-service attacks.
 *
 * # Example
 *
 * ```javascript
 * const { safeLoadAll } = require('@fast-yaml/core');
 *
 * const docs = safeLoadAll('---
foo: 1
---
bar: 2');
 * console.log(docs); // [{ foo: 1 }, { bar: 2 }]
 * ```
 */
export declare function safeLoadAll(yamlStr: string, options?: LoadOptions | undefined | null): Array<unknown>

/**
 * YAML schema types for parsing behavior (js-yaml compatible).
 *
 * All schemas currently behave as `SAFE_SCHEMA` (safe by default).
 * The schema parameter is accepted for API compatibility with js-yaml.
 */
export declare const enum Schema {
  /**
   * Safe schema - only safe data types (default).
   * Equivalent to `PyYAML`'s `SafeLoader`.
   */
  SafeSchema = 'SafeSchema',
  /** JSON schema - strict JSON subset of YAML. */
  JsonSchema = 'JsonSchema',
  /** Core schema - YAML 1.2.2 Core Schema. */
  CoreSchema = 'CoreSchema',
  /** Failsafe schema - minimal safe subset. */
  FailsafeSchema = 'FailsafeSchema'
}

/** Diagnostic severity levels. */
export declare const enum Severity {
  /** Critical error that prevents YAML parsing or violates spec. */
  Error = 'Error',
  /** Potential issue that should be addressed. */
  Warning = 'Warning',
  /** Informational message about style or best practices. */
  Info = 'Info',
  /** Suggestion for improvement. */
  Hint = 'Hint'
}

/** A span of text in the source file. */
export interface Span {
  /** Start position (inclusive). */
  start: Location
  /** End position (exclusive). */
  end: Location
}

/** A suggested fix for a diagnostic. */
export interface Suggestion {
  /** Description of the fix. */
  message: string
  /** Span to replace. */
  span: Span
  /** Replacement text (None = deletion). */
  replacement?: string
}

/**
 * Get the library version.
 *
 * Returns the version string of the fast-yaml-nodejs crate.
 *
 * # Examples
 *
 * ```javascript
 * const { version } = require('@fast-yaml/core');
 * console.log(version()); // "0.1.0"
 * ```
 */
export declare function version(): string
