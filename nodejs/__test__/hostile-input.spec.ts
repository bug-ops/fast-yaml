import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  formatFiles,
  formatFilesInPlace,
  parseParallel,
  processFiles,
  safeDump,
  safeLoad,
  safeLoadAll,
} from '../index.js';

const BOM = '\ufeff';

describe('loaded objects', () => {
  it('keeps __proto__ as an own property instead of replacing the prototype', () => {
    const loaded = safeLoad('__proto__:\n  isAdmin: true\nk: 1') as Record<string, unknown>;
    expect(Object.getPrototypeOf(loaded)).toBe(Object.prototype);
    expect((loaded as { isAdmin?: boolean }).isAdmin).toBeUndefined();
    expect(Object.hasOwn(loaded, '__proto__')).toBe(true);
    expect(Object.keys(loaded)).toEqual(['__proto__', 'k']);
  });

  it('keeps __proto__ as an own property of a set', () => {
    const loaded = safeLoad('!!set {__proto__, a}') as Record<string, unknown>;
    expect(Object.getPrototypeOf(loaded)).toBe(Object.prototype);
    expect(Object.keys(loaded)).toEqual(['__proto__', 'a']);
  });

  it('prints colliding keys quoted', () => {
    let message = '';
    try {
      safeLoad(`1: a\n"1": b\n`);
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toContain('"1"');
    expect(message).not.toContain('\u001b');
  });
});

describe('dump round trips', () => {
  it.each([' admin', '\u00a0admin', '\u3000admin', '\u0085admin', '\u2028admin'])(
    'treats %j at the start of a key as data for any indent',
    (key) => {
      for (const indent of [3, 4, 9]) {
        const data = { user: { name: 'bob' }, [key]: true };
        expect(safeLoad(safeDump(data, { indent }))).toEqual(data);
      }
    }
  );

  it('quotes strings holding a BOM', () => {
    for (const data of [{ [`${BOM}admin`]: `a${BOM}b`, k: BOM }, `${BOM}root`, [BOM]]) {
      expect(safeLoad(safeDump(data))).toEqual(data);
      expect(safeLoad(safeDump(data, { defaultFlowStyle: true }))).toEqual(data);
    }
  });
});

describe('batch input', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hostile-input-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('keeps a leading BOM when formatting', () => {
    const file = path.join(tmpDir, 'bom.yaml');
    fs.writeFileSync(file, `${BOM}# c\na:   1\n`);
    const [result] = formatFiles([file]);
    expect(result.error).toBeUndefined();
    expect(result.content).toBe(`${BOM}a: 1\n`);
  });

  it('leaves a formatted BOM file unchanged in place', () => {
    const file = path.join(tmpDir, 'bom.yaml');
    fs.writeFileSync(file, `${BOM}a: 1\n`);
    expect(formatFilesInPlace([file]).changed).toBe(0);
    expect(fs.readFileSync(file, 'utf8')).toBe(`${BOM}a: 1\n`);
  });

  it('reports BOM-less UTF-16 and UTF-32 as unsupported', () => {
    const utf16le = Buffer.from('a: 1\n', 'utf16le');
    const utf16be = Buffer.from(utf16le).swap16();
    const utf32le = Buffer.concat(
      [...'a: 1\n'].map((c) => Buffer.from([c.charCodeAt(0), 0, 0, 0]))
    );
    const utf32be = Buffer.concat(
      [...'a: 1\n'].map((c) => Buffer.from([0, 0, 0, c.charCodeAt(0)]))
    );
    for (const [name, data] of [
      ['utf16le', utf16le],
      ['utf16be', utf16be],
      ['utf32le', utf32le],
      ['utf32be', utf32be],
    ] as const) {
      const file = path.join(tmpDir, `${name}.yaml`);
      fs.writeFileSync(file, data);
      const processed = processFiles([file]);
      expect(processed.failed, name).toBe(1);
      expect(processed.errors[0].message.toLowerCase(), name).toContain('unsupported encoding');
      const [formatted] = formatFiles([file]);
      expect(formatted.content, name).toBeUndefined();
      expect(formatted.error?.toLowerCase(), name).toContain('unsupported encoding');
    }
  });
});

describe('rejected characters', () => {
  it.each([
    ['a: \u0001\n', undefined],
    ['a: 1\n---\nb: \u0001\n', '(document 2)'],
    ['a: 1\n---\nb: 2\n---\nc: \u0001', '(document 3)'],
    ['﻿a: 1\n...\n﻿b: \u007f\n', '(document 2)'],
  ])('report their document for %j', (text, marker) => {
    for (const load of [safeLoadAll, parseParallel]) {
      let message = '';
      try {
        load(text);
      } catch (error) {
        message = (error as Error).message;
      }
      expect(message).toContain('not allowed in YAML');
      if (marker) {
        expect(message).toContain(marker);
      } else {
        expect(message).not.toContain('(document');
      }
    }
  });
});

describe('nested collections at any indent', () => {
  it.each([1, 2, 3, 4, 5, 9])('keeps nested lists for indent %i', (indent) => {
    const data = {
      perms: [
        ['read', 'write'],
        [true, 13],
        [[1], []],
      ],
      m: [{ k: ['v'] }],
    };
    expect(safeLoad(safeDump(data, { indent }))).toEqual(data);
  });
});
