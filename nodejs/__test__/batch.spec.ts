import { afterEach, beforeEach, describe, expect, it } from "vitest";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type {
	BatchConfig,
	BatchResult,
	FormatResult,
} from "../index.js";
import {
	FileOutcome,
	formatFiles,
	formatFilesInPlace,
	processFiles,
} from "../index.js";

describe("Batch Processing", () => {
	let tmpDir: string;
	let testFiles: string[];

	beforeEach(() => {
		tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "batch-test-"));
		testFiles = [];
		for (let i = 0; i < 5; i++) {
			const filePath = path.join(tmpDir, `file${i}.yaml`);
			fs.writeFileSync(filePath, `key${i}: value${i}\n`);
			testFiles.push(filePath);
		}
	});

	afterEach(() => {
		fs.rmSync(tmpDir, { recursive: true, force: true });
	});

	describe("FileOutcome enum", () => {
		it("should have all outcome values", () => {
			expect(FileOutcome.Success).toBe("Success");
			expect(FileOutcome.Changed).toBe("Changed");
			expect(FileOutcome.Unchanged).toBe("Unchanged");
			expect(FileOutcome.Error).toBe("Error");
		});
	});

	describe("processFiles", () => {
		it("should process valid files", () => {
			const result = processFiles(testFiles);
			expect(result.total).toBe(5);
			expect(result.success).toBe(5);
			expect(result.failed).toBe(0);
			expect(result.errors).toHaveLength(0);
		});

		it("should handle invalid files", () => {
			const invalidPath = path.join(tmpDir, "invalid.yaml");
			fs.writeFileSync(invalidPath, "invalid: [\n");

			const result = processFiles([...testFiles, invalidPath]);
			expect(result.total).toBe(6);
			expect(result.success).toBe(5);
			expect(result.failed).toBe(1);
			expect(result.errors).toHaveLength(1);
		});

		it("should handle empty array", () => {
			const result = processFiles([]);
			expect(result.total).toBe(0);
			expect(result.failed).toBe(0);
		});

		it("should accept config", () => {
			const config: BatchConfig = { workers: 2 };
			const result = processFiles(testFiles, config);
			expect(result.success).toBe(5);
		});

		it("should handle nonexistent files", () => {
			const result = processFiles(["/nonexistent/file.yaml"]);
			expect(result.total).toBe(1);
			expect(result.failed).toBe(1);
		});

		it("should calculate duration", () => {
			const result = processFiles(testFiles);
			expect(result.durationMs).toBeGreaterThanOrEqual(0);
		});
	});

	describe("formatFiles", () => {
		it("should format valid files", () => {
			const results = formatFiles(testFiles);
			expect(results).toHaveLength(5);
			for (const r of results) {
				expect(r.content).toBeDefined();
				expect(r.error).toBeUndefined();
			}
		});

		it("should handle invalid files", () => {
			const invalidPath = path.join(tmpDir, "invalid.yaml");
			fs.writeFileSync(invalidPath, "invalid: [\n");

			const results = formatFiles([invalidPath]);
			expect(results).toHaveLength(1);
			expect(results[0].content).toBeUndefined();
			expect(results[0].error).toBeDefined();
		});

		it("should handle empty array", () => {
			const results = formatFiles([]);
			expect(results).toHaveLength(0);
		});

		it("should accept config", () => {
			const config: BatchConfig = { indent: 4, sortKeys: true };
			const results = formatFiles(testFiles, config);
			expect(results).toHaveLength(5);
		});

		it("should not modify files", () => {
			const unformattedPath = path.join(tmpDir, "unformatted.yaml");
			fs.writeFileSync(unformattedPath, "key:     value\n");
			const original = fs.readFileSync(unformattedPath, "utf-8");

			formatFiles([unformattedPath]);

			const after = fs.readFileSync(unformattedPath, "utf-8");
			expect(after).toBe(original);
		});
	});

	describe("formatFilesInPlace", () => {
		it("should format files in place", () => {
			const result = formatFilesInPlace(testFiles);
			expect(result.total).toBe(5);
			expect(result.success).toBe(5);
			expect(result.failed).toBe(0);
		});

		it("should track changed files", () => {
			const unformattedPath = path.join(tmpDir, "unformatted.yaml");
			fs.writeFileSync(unformattedPath, "key:     value\n");

			const result = formatFilesInPlace([unformattedPath]);
			expect(result.total).toBe(1);
			expect(result.success).toBe(1);
		});

		it("should handle empty array", () => {
			const result = formatFilesInPlace([]);
			expect(result.total).toBe(0);
		});

		it("should accept config", () => {
			const config: BatchConfig = { indent: 4 };
			const result = formatFilesInPlace(testFiles, config);
			expect(result.success).toBe(5);
		});

		it("should handle nonexistent files", () => {
			const result = formatFilesInPlace(["/nonexistent/file.yaml"]);
			expect(result.total).toBe(1);
			expect(result.failed).toBe(1);
		});
	});

	describe("BatchConfig", () => {
		it("should accept all options", () => {
			const config: BatchConfig = {
				workers: 4,
				mmapThreshold: 1024 * 1024,
				maxInputSize: 50 * 1024 * 1024,
				sequentialThreshold: 2048,
				indent: 4,
				width: 120,
				sortKeys: true,
			};
			const result = processFiles(testFiles, config);
			expect(result.success).toBe(5);
		});










	});

	describe("Edge cases", () => {
		it("should handle unicode content", () => {
			const unicodePath = path.join(tmpDir, "unicode.yaml");
			fs.writeFileSync(unicodePath, "chinese: 中文\n", "utf-8");
			const result = processFiles([unicodePath]);
			expect(result.success).toBe(1);
		});

		it("should handle large files", () => {
			const largePath = path.join(tmpDir, "large.yaml");
			const content = "key: value\n".repeat(100_000);
			fs.writeFileSync(largePath, content);
			const result = processFiles([largePath]);
			expect(result.success).toBe(1);
		});

		it("should handle many files", () => {
			const manyFiles: string[] = [];
			for (let i = 0; i < 100; i++) {
				const filePath = path.join(tmpDir, `many${i}.yaml`);
				fs.writeFileSync(filePath, `id: ${i}\n`);
				manyFiles.push(filePath);
			}
			const result = processFiles(manyFiles);
			expect(result.total).toBe(100);
			expect(result.success).toBe(100);
		});

		it("should handle empty files", () => {
			const emptyPath = path.join(tmpDir, "empty.yaml");
			fs.writeFileSync(emptyPath, "");
			const result = processFiles([emptyPath]);
			expect(result.total).toBe(1);
		});

		it("should handle paths with spaces", () => {
			const spacePath = path.join(tmpDir, "file with spaces.yaml");
			fs.writeFileSync(spacePath, "key: value\n");
			const result = processFiles([spacePath]);
			expect(result.success).toBe(1);
		});
	});
});
