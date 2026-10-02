import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  build: {
    sourcemap: false,
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
