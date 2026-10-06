import { defineConfig } from '@playwright/test';

const origin = process.env.OCTACITY_RELEASE_CONSOLE_ORIGIN;
if (origin === undefined) {
  throw new Error('OCTACITY_RELEASE_CONSOLE_ORIGIN must identify the released same-origin proxy');
}

export default defineConfig({
  forbidOnly: true,
  fullyParallel: false,
  reporter: 'list',
  testDir: './tests/released',
  use: {
    baseURL: origin,
    colorScheme: 'light',
    locale: 'en-US',
    reducedMotion: 'reduce',
    serviceWorkers: 'block',
    timezoneId: 'UTC',
    trace: 'retain-on-failure',
    viewport: { height: 720, width: 1_280 },
  },
  workers: 1,
});
