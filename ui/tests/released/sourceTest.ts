import { expect, test as base } from '@playwright/test';

const canaries = ['packaged-fixture-reader-canary', 'packaged-fixture-writer-canary'];

/** Credentials are provisioned over protected setup only; public/browser state must omit them. */
export const test = base.extend({
  page: async ({ page }, use) => {
    const publicResponses: Promise<string | null>[] = [];
    page.on('response', (response) => {
      if (new URL(response.url()).pathname.startsWith('/api/v1/')) {
        // A cancelled response has no retained body to inspect. Business assertions still
        // require successful responses at each journey's relevant command/query boundary.
        publicResponses.push(response.text().catch(() => null));
      }
    });
    await use(page);
    const storage = await page.evaluate(() =>
      JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
    );
    for (const body of [storage, ...(await Promise.all(publicResponses))]) {
      if (body === null) continue;
      for (const canary of canaries) expect(body).not.toContain(canary);
    }
  },
});
