/**
 * Unit tests for Mark class (error location tracking)
 */

import { describe, expect, it } from 'vitest';
import { Mark } from '../index';

describe('Mark Class', () => {
  describe('constructor', () => {
    it('should create a Mark with all parameters', () => {
      const mark = new Mark('<input>', 10, 5);
      expect(mark.name).toBe('<input>');
      expect(mark.line).toBe(10);
      expect(mark.column).toBe(5);
    });

    it('should create a Mark with filename', () => {
      const mark = new Mark('config.yaml', 42, 15);
      expect(mark.name).toBe('config.yaml');
      expect(mark.line).toBe(42);
      expect(mark.column).toBe(15);
    });

    it('should create a Mark with zero indices', () => {
      const mark = new Mark('test.yaml', 0, 0);
      expect(mark.name).toBe('test.yaml');
      expect(mark.line).toBe(0);
      expect(mark.column).toBe(0);
    });

    it('should create a Mark with large line numbers', () => {
      const mark = new Mark('large.yaml', 999999, 888888);
      expect(mark.name).toBe('large.yaml');
      expect(mark.line).toBe(999999);
      expect(mark.column).toBe(888888);
    });

    it('should create a Mark with path separator in filename', () => {
      const mark = new Mark('/path/to/file.yaml', 5, 10);
      expect(mark.name).toBe('/path/to/file.yaml');
      expect(mark.line).toBe(5);
      expect(mark.column).toBe(10);
    });

    it('should create a Mark with Windows-style path', () => {
      const mark = new Mark('C:\\Users\\test\\config.yaml', 15, 20);
      expect(mark.name).toBe('C:\\Users\\test\\config.yaml');
      expect(mark.line).toBe(15);
      expect(mark.column).toBe(20);
    });

    it('should create a Mark with relative path', () => {
      const mark = new Mark('./config/app.yaml', 3, 7);
      expect(mark.name).toBe('./config/app.yaml');
      expect(mark.line).toBe(3);
      expect(mark.column).toBe(7);
    });

    it('should create a Mark with empty name', () => {
      const mark = new Mark('', 5, 10);
      expect(mark.name).toBe('');
      expect(mark.line).toBe(5);
      expect(mark.column).toBe(10);
    });

    it('should create a Mark with special characters in name', () => {
      const mark = new Mark('<stdin>', 1, 1);
      expect(mark.name).toBe('<stdin>');
      expect(mark.line).toBe(1);
      expect(mark.column).toBe(1);
    });

    it('should create a Mark with URL as name', () => {
      const mark = new Mark('https://example.com/config.yaml', 10, 20);
      expect(mark.name).toBe('https://example.com/config.yaml');
      expect(mark.line).toBe(10);
      expect(mark.column).toBe(20);
    });
  });

  describe('properties', () => {
    it('should have readonly name property', () => {
      const mark = new Mark('test.yaml', 5, 10);
      expect(mark.name).toBe('test.yaml');
    });

    it('should have readonly line property', () => {
      const mark = new Mark('test.yaml', 5, 10);
      expect(mark.line).toBe(5);
    });

    it('should have readonly column property', () => {
      const mark = new Mark('test.yaml', 5, 10);
      expect(mark.column).toBe(10);
    });

    it('should allow reading all properties', () => {
      const mark = new Mark('file.yaml', 100, 50);
      const { name, line, column } = mark;
      expect(name).toBe('file.yaml');
      expect(line).toBe(100);
      expect(column).toBe(50);
    });
  });

  describe('toString()', () => {
    it('should return formatted string with name:line:column', () => {
      const mark = new Mark('test.yaml', 42, 15);
      expect(mark.toString()).toBe('test.yaml:42:15');
    });

    it('should return formatted string with zero indices', () => {
      const mark = new Mark('file.yaml', 0, 0);
      expect(mark.toString()).toBe('file.yaml:0:0');
    });

    it('should return formatted string with large numbers', () => {
      const mark = new Mark('big.yaml', 123456, 789012);
      expect(mark.toString()).toBe('big.yaml:123456:789012');
    });

    it('should return formatted string with <input> name', () => {
      const mark = new Mark('<input>', 5, 10);
      expect(mark.toString()).toBe('<input>:5:10');
    });

    it('should return formatted string with path', () => {
      const mark = new Mark('/etc/config.yaml', 20, 35);
      expect(mark.toString()).toBe('/etc/config.yaml:20:35');
    });

    it('should return formatted string with empty name', () => {
      const mark = new Mark('', 1, 2);
      expect(mark.toString()).toBe(':1:2');
    });

    it('should return formatted string with single digit indices', () => {
      const mark = new Mark('a.yaml', 1, 2);
      expect(mark.toString()).toBe('a.yaml:1:2');
    });

    it('should return formatted string that matches error message format', () => {
      const mark = new Mark('config.yaml', 10, 5);
      const str = mark.toString();
      expect(str).toMatch(/^[^:]+:\d+:\d+$/);
    });
  });

  describe('multiple Mark instances', () => {
    it('should create independent Mark instances', () => {
      const mark1 = new Mark('file1.yaml', 10, 5);
      const mark2 = new Mark('file2.yaml', 20, 15);

      expect(mark1.name).toBe('file1.yaml');
      expect(mark1.line).toBe(10);
      expect(mark1.column).toBe(5);

      expect(mark2.name).toBe('file2.yaml');
      expect(mark2.line).toBe(20);
      expect(mark2.column).toBe(15);
    });

    it('should allow creating many Mark instances', () => {
      const marks = [
        new Mark('a.yaml', 1, 1),
        new Mark('b.yaml', 2, 2),
        new Mark('c.yaml', 3, 3),
        new Mark('d.yaml', 4, 4),
        new Mark('e.yaml', 5, 5),
      ];

      expect(marks).toHaveLength(5);
      expect(marks[0].toString()).toBe('a.yaml:1:1');
      expect(marks[4].toString()).toBe('e.yaml:5:5');
    });

    it('should maintain separate state for each instance', () => {
      const marks = Array(100)
        .fill(null)
        .map((_, i) => new Mark(`file${i}.yaml`, i, i * 2));

      expect(marks[0].line).toBe(0);
      expect(marks[0].column).toBe(0);
      expect(marks[50].line).toBe(50);
      expect(marks[50].column).toBe(100);
      expect(marks[99].line).toBe(99);
      expect(marks[99].column).toBe(198);
    });
  });

  describe('usage in error reporting', () => {
    it('should be useful for tracking parse error locations', () => {
      const mark = new Mark('config.yaml', 15, 10);
      const errorMessage = `YAML parse error at ${mark.toString()}`;
      expect(errorMessage).toBe('YAML parse error at config.yaml:15:10');
    });

    it('should provide context for multiple errors', () => {
      const errors = [
        { mark: new Mark('app.yaml', 10, 5), message: 'Invalid syntax' },
        { mark: new Mark('app.yaml', 20, 15), message: 'Duplicate key' },
        { mark: new Mark('app.yaml', 30, 8), message: 'Unknown type' },
      ];

      expect(errors[0].mark.toString()).toBe('app.yaml:10:5');
      expect(errors[1].mark.toString()).toBe('app.yaml:20:15');
      expect(errors[2].mark.toString()).toBe('app.yaml:30:8');
    });

    it('should support IDE integration with line:column format', () => {
      const mark = new Mark('/project/src/config.yaml', 42, 15);
      const ideFormat = mark.toString();
      expect(ideFormat).toMatch(/^.*:\d+:\d+$/);
    });
  });

  describe('edge cases', () => {
    it('should handle maximum u32 value for line', () => {
      const maxU32 = 4294967295;
      const mark = new Mark('test.yaml', maxU32, 0);
      expect(mark.line).toBe(maxU32);
    });

    it('should handle maximum u32 value for column', () => {
      const maxU32 = 4294967295;
      const mark = new Mark('test.yaml', 0, maxU32);
      expect(mark.column).toBe(maxU32);
    });

    it('should handle very long filename', () => {
      const longName = `${'a'.repeat(1000)}.yaml`;
      const mark = new Mark(longName, 5, 10);
      expect(mark.name).toBe(longName);
      expect(mark.name.length).toBe(1005);
    });

    it('should handle unicode in filename', () => {
      const mark = new Mark('配置文件.yaml', 10, 5);
      expect(mark.name).toBe('配置文件.yaml');
      expect(mark.toString()).toBe('配置文件.yaml:10:5');
    });

    it('should handle emoji in filename', () => {
      const mark = new Mark('config🔧.yaml', 1, 2);
      expect(mark.name).toBe('config🔧.yaml');
      expect(mark.toString()).toBe('config🔧.yaml:1:2');
    });

    it('should handle newlines in filename (unusual but possible)', () => {
      const mark = new Mark('line1\nline2.yaml', 5, 10);
      expect(mark.name).toBe('line1\nline2.yaml');
    });

    it('should handle tab characters in filename', () => {
      const mark = new Mark('file\twith\ttabs.yaml', 3, 7);
      expect(mark.name).toBe('file\twith\ttabs.yaml');
    });
  });
});
