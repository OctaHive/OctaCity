import eslintReact from '@eslint-react/eslint-plugin';
import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import tseslint from 'typescript-eslint';

export default defineConfig(
  globalIgnores(['dist/', 'coverage/', 'playwright-report/', 'test-results/', '.generated/']),
  {
    files: ['**/*.{js,cjs,mjs}'],
    extends: [js.configs.recommended],
  },
  {
    files: ['**/*.{ts,tsx}'],
    extends: [js.configs.recommended, tseslint.configs.recommended],
    rules: {
      // TypeScript resolves names against the configured DOM and Node libraries.
      'no-undef': 'off',
    },
  },
  {
    files: ['src/**/*.{ts,tsx}'],
    extends: [eslintReact.configs['recommended-typescript']],
  },
);
