import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });

test('copied Factory Run filters survive search, collapse and resize, and the visible Project opens the guided workbench', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto(`/factory/${FIXTURE_IDS.factoryRun}?state=waiting&source=manual`);
  await expect(page.getByRole('heading', { name: 'Factory Run', level: 1 })).toBeVisible();
  const explorer = page.getByRole('complementary', { name: 'Factory explorer' });
  await expect(explorer.getByRole('link', { name: 'Requested change' })).toHaveAttribute(
    'href',
    `/factory/${FIXTURE_IDS.factoryRun}?state=waiting&source=manual&configuration_id=22222222-2222-4222-8222-222222222222`,
  );
  await page.getByRole('button', { name: 'Search Factory' }).click();
  await expect(explorer.locator('[data-factory-filters]')).toBeFocused();
  await page.getByRole('button', { name: 'Collapse Factory explorer' }).click();
  await page.getByRole('button', { name: 'Open Factory explorer' }).click();
  await expect(explorer.getByRole('button', { name: 'Collapse Factory Project' })).toBeVisible();
  const resize = page.getByRole('separator', { name: 'Resize Factory explorer' });
  await resize.focus();
  await page.keyboard.press('ArrowRight');
  await expect(explorer.getByRole('link', { name: 'Requested change' })).toBeVisible();
  await page.getByRole('button', { name: 'Add to Favorites' }).click();
  await expect(page.getByRole('button', { name: 'Remove from Favorites' })).toBeVisible();
  await page.getByRole('link', { name: 'Open Project Factory workbench' }).click();
  await expect(
    page.getByRole('main').getByRole('heading', { level: 1, name: 'Factory' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Inspect Flow Definition · v1' }).click();
  await page.getByRole('button', { name: 'Edit Flow draft' }).click();
  await expect(page.getByRole('textbox', { name: 'Node name' })).toHaveValue('implement');
  await page.getByRole('heading', { name: 'Flow graph editor' }).scrollIntoViewIfNeeded();
  await page.getByRole('textbox', { name: 'Node name' }).fill('visible_operator_draft');
  expect(page.url()).not.toContain('visible_operator_draft');
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(
    'visible_operator_draft',
  );
  await page.getByRole('button', { name: 'Discard draft' }).click();
});

test('keyboard confirmation can be abandoned and reopened with focus restored', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto(`/factory/${FIXTURE_IDS.factoryRun}`);
  const command = page.getByRole('button', { name: 'Request cancellation' });
  await command.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Confirm Run command?' });
  await expect(dialog.getByRole('button', { name: 'Confirm command' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(command).toBeFocused();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await expect(dialog.getByText('Cancellation requested.', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('link', { name: 'View audit evidence' })).toHaveAttribute(
    'href',
    `/factory/${FIXTURE_IDS.factoryRun}?diagnostic=audit`,
  );
});

test('manual Work is discarded on reload and leave, and confirmed visible inputs open the returned Run', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const owner = configurationFixture().configuration.reference.project_id;
  const path = `/factory?project_id=${owner}`;
  await page.goto(path);
  await page.getByRole('button', { name: 'Admit Work', exact: true }).click();
  const label = page.getByRole('textbox', { name: 'Work label' });
  await label.fill('private unfinished work');
  expect(page.url()).not.toContain('private');
  expect(
    await page.evaluate(() => `${JSON.stringify(localStorage)}${JSON.stringify(sessionStorage)}`),
  ).not.toContain('private unfinished work');
  await page.reload();
  await page.getByRole('button', { name: 'Admit Work', exact: true }).click();
  await expect(label).toHaveValue('');
  await label.fill('private unfinished work');
  await page.getByRole('link', { name: 'Projects', exact: true }).first().click();
  await page.goto(path);
  await page.getByRole('button', { name: 'Admit Work', exact: true }).click();
  await expect(label).toHaveValue('');
  await label.fill('Synthetic Work');
  await page.getByRole('combobox', { name: 'Task Artifact' }).click();
  await page.getByRole('option', { name: 'Task input' }).click();
  await page.getByRole('combobox', { name: 'Acceptance Artifact' }).click();
  await page.getByRole('option', { name: 'Acceptance input' }).click();
  const review = page.getByRole('button', { name: 'Review Work admission' });
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Admit this Work?' });
  await expect(dialog.getByText(/Factory source · v1/)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(new RegExp(`/factory/${FIXTURE_IDS.factoryRun}$`));
});

test('Run diagnostics keep graph and table available at narrow width with reduced motion', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.setViewportSize({ height: 900, width: 640 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto(`/factory/${FIXTURE_IDS.factoryRun}`);
  await expect(page.getByRole('table', { name: 'Nested Flow Runs' })).toBeVisible();
  await expect(page.getByRole('img', { name: 'Nested Flow Runs' })).toBeVisible();
  const pinned = page.getByRole('table', { name: 'Pinned nodes' });
  await expect(pinned).not.toBeVisible();
  const closure = page.locator('summary').filter({ hasText: 'Pinned definition closure' });
  await closure.focus();
  await page.keyboard.press('Enter');
  await expect(pinned).toBeVisible();
  await expect(
    page.getByText('Read-only pinned graph. The table lists the same declared routes.'),
  ).toBeVisible();
  await expect(page.getByRole('img', { name: 'Flow graph' })).toBeVisible();
  expect(
    await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
    ),
  ).toBe(0);
});

test('Factory provider failure leaves ordinary CI/CD navigation available', async ({ page }) => {
  await installOperatorFixture(page);
  await page.route('**/api/v1/factory-runs/*/node-attempts*', (route) =>
    route.fulfill({
      status: 503,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 'unavailable',
        message: 'Fixture provider unavailable',
        request_id: 'provider-correlation',
      }),
    }),
  );
  await page.goto(`/factory/${FIXTURE_IDS.factoryRun}`);
  await expect(page.getByText('Fixture provider unavailable')).toBeVisible();
  await expect(page.getByRole('status', { name: 'Server ready' })).toBeVisible();
  await page.getByRole('link', { name: 'Builds', exact: true }).first().click();
  await expect(
    page.getByRole('main').getByRole('heading', { level: 1, name: 'Builds', exact: true }),
  ).toBeVisible();
});

test('offers existing manual Artifact admission from the selected pipeline without an integration setup', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const owner = configurationFixture().configuration.reference.project_id;
  const integrations: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/v1/integrations'))
      integrations.push(request.url());
  });
  await page.goto(`/factory?project_id=${owner}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await expect(page.getByRole('heading', { name: 'Pipeline inputs' })).toBeVisible();
  await page.getByRole('button', { name: 'Admit manual Work' }).click();
  await expect(page.getByRole('combobox', { name: 'Task Artifact' })).toBeVisible();
  expect(integrations).toEqual([]);
});
