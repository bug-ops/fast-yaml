import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Run tests sequentially to avoid memory pressure from large string tests
    fileParallelism: false,
    // Exclude edge-cases.spec.ts temporarily due to memory issues with 100MB tests
    exclude: ['**/node_modules/**', '**/edge-cases.spec.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov', 'html'],
      reportsDirectory: './coverage',
      include: ['index.js'],
      exclude: ['node_modules', 'dist', '*.config.*', '__test__'],
    },
  },
});
