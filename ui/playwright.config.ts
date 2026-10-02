import { defineConfig } from '@playwright/test';

const CONSOLE_ORIGIN = 'http://127.0.0.1:4173';

export default defineConfig({
  forbidOnly: process.env.CI === 'true',
  fullyParallel: true,
  reporter: 'list',
  testDir: './tests/browser',
  use: {
    baseURL: CONSOLE_ORIGIN,
    colorScheme: 'light',
    locale: 'en-US',
    reducedMotion: 'reduce',
    serviceWorkers: 'block',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'corepack pnpm dev --host 127.0.0.1 --port 4173',
    reuseExistingServer: process.env.CI !== 'true',
    url: CONSOLE_ORIGIN,
  },
});
