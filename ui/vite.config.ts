import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
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
  },
});
