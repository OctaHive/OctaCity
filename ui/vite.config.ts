import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig(({ command }) => ({
  plugins: [react()],
  // Compact production selectors without changing local class names or development diagnostics.
  css: command === 'build' ? { modules: { generateScopedName: 'c[hash:base64:8]' } } : {},
  build: {
    rolldownOptions: {
      output: {
        // Content hashes identify immutable chunks; keep repeated production import URLs compact.
        entryFileNames: 'assets/[hash].js',
        chunkFileNames: 'assets/[hash].js',
        assetFileNames: 'assets/[hash][extname]',
        codeSplitting: {
          groups: [
            // Reduce shared-control chunk overhead while preserving Factory's composed CSS order.
            {
              name: 'console-shared',
              test: /src\/shared\/(?!Factory)/u,
              minShareCount: 2,
              includeDependenciesRecursively: false,
            },
            {
              name: 'message-catalogs',
              test: /src\/app\/presentation\/messages\.(?:en|ru)\.ts$/u,
            },
          ],
        },
      },
    },
    sourcemap: false,
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    maxWorkers: 4,
    setupFiles: ['./tests/setup.ts'],
    testTimeout: 15_000,
  },
}));
