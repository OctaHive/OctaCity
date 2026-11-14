import { expect, type APIResponse, type Page } from '@playwright/test';
import { translate } from '../../src/app/presentation/messages';

/** The caller first proves that the sibling Issue's authority and revision are usable. */
export async function expectForeignMaterialRejected(response: APIResponse) {
  expect(response.status()).toBe(400);
  expect(await response.json()).toMatchObject({ code: 'invalid_request' });
}

/** Review through the same visible start form used by the packaged journey. */
export async function reviewIssueSnapshotLaunch(page: Page) {
  // Check the visible form before the modal makes its background inert.
  await expect(
    page.getByRole('combobox', {
      name: translate('en', 'issues.predecessorRun'),
      exact: true,
    }),
  ).toHaveCount(0);
  await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  return dialog;
}
