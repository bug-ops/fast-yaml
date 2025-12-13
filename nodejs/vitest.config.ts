import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov', 'html'],
      reportsDirectory: './coverage',
      include: ['__test__/**/*.ts'],
      exclude: ['node_modules', 'dist', '*.config.*'],
    },
  },
});
