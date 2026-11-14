import { createServer } from 'node:http';
import { expect, test } from '@playwright/test';
import {
  expectForeignMaterialRejected,
  reviewIssueSnapshotLaunch,
} from '../released/issueMaterialChecks';

const replies: Record<string, { status: number; code: string }> = {
  '/ownership': { status: 400, code: 'invalid_request' },
  '/server-error': { status: 500, code: 'internal_error' },
  '/forbidden': { status: 403, code: 'forbidden' },
  '/conflict': { status: 409, code: 'conflict' },
  '/other-invalid': { status: 400, code: 'unsupported_api_version' },
};
const server = createServer((request, response) => {
  const reply = replies[request.url ?? ''];
  response.writeHead(reply?.status ?? 404, { 'Content-Type': 'application/json' });
  response.end(
    JSON.stringify({ code: reply?.code ?? 'not_found', message: 'qualification fixture' }),
  );
});
let origin: string;
test.beforeAll(async () => {
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('Expected loopback HTTP');
  origin = `http://127.0.0.1:${address.port}`;
});
test.afterAll(async () => {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
});

test('recognizes the public invalid-material response', async ({ request }) => {
  await expectForeignMaterialRejected(await request.get(`${origin}/ownership`));
});

for (const path of ['/server-error', '/forbidden', '/conflict', '/other-invalid']) {
  test(`does not count ${path} as an ownership rejection`, async ({ request }) => {
    const response = await request.get(origin + path);
    await expect(expectForeignMaterialRejected(response)).rejects.toThrow();
  });
}

for (const earlierRun of [false, true]) {
  test(`snapshot review ${earlierRun ? 'rejects an earlier-run selector in the start form' : 'opens without a producer selector'}`, async ({
    page,
  }) => {
    await page.setContent(`
      <form id="start">
        <select aria-label="Source"><option>Exact revision</option></select>
        ${earlierRun ? '<select aria-label="Earlier Issue run"><option>Run 1</option></select>' : ''}
        <button type="button" id="review">Review pipeline launch</button>
      </form>
      <dialog id="confirmation" aria-label="Review pipeline launch">Frozen Issue input</dialog>
      <script>
        document.getElementById('review').onclick = () => {
          document.getElementById('start').inert = true;
          document.getElementById('confirmation').showModal();
        };
      </script>`);
    if (earlierRun) {
      await expect(reviewIssueSnapshotLaunch(page)).rejects.toThrow();
      await expect(page.getByRole('dialog')).toHaveCount(0);
    } else {
      await expect(await reviewIssueSnapshotLaunch(page)).toBeVisible();
    }
  });
}
