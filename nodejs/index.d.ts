/**
 * Fast YAML 1.2.2 parser for Node.js, powered by Rust
 *
 * This module provides high-performance YAML parsing and serialization
 * with 5-10x speedup over pure JavaScript implementations.
 *
 * @module @fast-yaml/core
 */

/**
 * Options for YAML serialization.
 */
export interface DumpOptions {
  /**
   * If true, sort object keys alphabetically (default: false)
   */
  sortKeys?: boolean;

  /**
   * Allow unicode characters (default: true)
   * Note: yaml-rust2 always outputs unicode; this is accepted for API compatibility
   */
  allowUnicode?: boolean;
}

/**
 * Parse a YAML string and return a JavaScript object.
 * Equivalent to js-yaml's `safeLoad()`.
 *
 * @param yamlStr - YAML string to parse
 * @returns Parsed JavaScript object
 * @throws {Error} If YAML is invalid or exceeds 100MB limit
 *
 * @example
 * ```typescript
 * import { safeLoad } from '@fast-yaml/core';
 *
 * const data = safeLoad('name: test\nvalue: 123');
 * console.log(data); // { name: 'test', value: 123 }
 * ```
 */
export function safeLoad(yamlStr: string): unknown;

/**
 * Parse a YAML string containing multiple documents.
 * Equivalent to js-yaml's `safeLoadAll()`.
 *
 * @param yamlStr - YAML string potentially containing multiple documents
 * @returns Array of parsed JavaScript objects
 * @throws {Error} If YAML is invalid or exceeds 100MB limit
 *
 * @example
 * ```typescript
 * import { safeLoadAll } from '@fast-yaml/core';
 *
 * const docs = safeLoadAll('---\nfoo: 1\n---\nbar: 2');
 * console.log(docs); // [{ foo: 1 }, { bar: 2 }]
 * ```
 */
export function safeLoadAll(yamlStr: string): unknown[];

/**
 * Serialize a JavaScript object to YAML string.
 * Equivalent to js-yaml's `safeDump()`.
 *
 * @param data - JavaScript object to serialize
 * @param options - Serialization options
 * @returns YAML string representation
 * @throws {TypeError} If object contains non-serializable types
 *
 * @example
 * ```typescript
 * import { safeDump } from '@fast-yaml/core';
 *
 * const yaml = safeDump({ name: 'test', value: 123 });
 * console.log(yaml); // 'name: test\nvalue: 123\n'
 * ```
 */
export function safeDump(data: unknown, options?: DumpOptions): string;

/**
 * Serialize multiple JavaScript objects to YAML with document separators.
 * Equivalent to js-yaml's `safeDumpAll()`.
 *
 * @param documents - Array of JavaScript objects to serialize
 * @param options - Serialization options
 * @returns YAML string with '---' separators
 * @throws {TypeError} If any object cannot be serialized
 * @throws {Error} If output exceeds 100MB limit
 *
 * @example
 * ```typescript
 * import { safeDumpAll } from '@fast-yaml/core';
 *
 * const yaml = safeDumpAll([{ a: 1 }, { b: 2 }]);
 * console.log(yaml); // '---\na: 1\n---\nb: 2\n'
 * ```
 */
export function safeDumpAll(documents: unknown[], options?: DumpOptions): string;

/**
 * Get the library version.
 *
 * @returns Version string (e.g., "0.1.0")
 *
 * @example
 * ```typescript
 * import { version } from '@fast-yaml/core';
 *
 * console.log(version()); // "0.1.0"
 * ```
 */
export function version(): string;

// Aliases for js-yaml compatibility
/**
 * Alias for safeLoad() for js-yaml compatibility.
 * @deprecated Use safeLoad() instead
 */
export { safeLoad as load };

/**
 * Alias for safeDump() for js-yaml compatibility.
 * @deprecated Use safeDump() instead
 */
export { safeDump as dump };
