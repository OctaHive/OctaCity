import { expect, type Page } from '@playwright/test';
import { test } from './sourceTest';
import { translate } from '../../src/app/presentation/messages';

const storage = process.env.OCTACITY_RELEASE_TRIAGE_STORAGE;
test.use(storage ? { storageState: storage } : {});

async function language(page: Page, current: 'en' | 'ru', next: 'en' | 'ru') {
  if (current === next) return;
  await page
    .getByRole('button', { name: translate(current, 'operator.menu'), exact: true })
    .press('Enter');
  await page
    .getByRole('combobox', { name: translate(current, 'language.label'), exact: true })
    .press('Enter');
  await page
    .getByRole('option', {
      name: translate(current, next === 'ru' ? 'language.russian' : 'language.english'),
      exact: true,
    })
    .press('Enter');
  await page.keyboard.press('Escape');
}

test('reads actual stage results, reproduction and separate delivery with narrow keyboard navigation in both locales', async ({
  page,
}) => {
  test.setTimeout(60_000);
  const run = process.env.OCTACITY_RELEASE_TRIAGE_RUN!;
  const writes: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/v1/') && request.method() !== 'GET')
      writes.push(request.url());
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/factory/${run}`);
  const resumed = process.env.OCTACITY_RELEASE_TRIAGE_PHASE === 'after-update';
  if (resumed) {
    await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  }
  let current: 'en' | 'ru' = resumed
    ? 'ru'
    : process.env.OCTACITY_RELEASE_TRIAGE_LOCALE === 'ru'
      ? 'ru'
      : 'en';
  for (const selected of ['en', 'ru'] as const) {
    const t = (key: Parameters<typeof translate>[1]) => translate(selected, key);
    await language(page, current, selected);
    current = selected;
    for (const key of [
      'triageResult.project',
      'triageResult.analyst',
      'triageResult.reporter',
    ] as const) {
      await expect(page.getByRole('heading', { name: t(key), exact: true })).toBeVisible();
    }
    await expect(page.locator('body')).toContainText(
      t(
        process.env.OCTACITY_RELEASE_TRIAGE_KIND === 'feature'
          ? 'triageResult.value.not_applicable'
          : 'triageResult.value.reproduced',
      ),
    );
    await expect(page.locator('body')).toContainText(t('triageResult.value.requirements'));
    await expect(page.locator('body')).toContainText(t('triageResult.delivery.delivered'));
    if (process.env.OCTACITY_RELEASE_TRIAGE_KIND === 'feature') {
      const task = page.getByRole('link', { name: t('factoryActions.task'), exact: true });
      const original = await task.getAttribute('href');
      expect(original).toContain('/issues/');
      await task.focus();
      await page.keyboard.press('Enter');
      await expect(
        page.getByRole('heading', { name: 'Feature: expose bounded triage evidence' }),
      ).toBeVisible();
      expect(page.url()).toContain(original!);
      await page.goBack();
      await expect(
        page.getByRole('heading', { name: t('triageResult.analyst'), exact: true }),
      ).toBeVisible();
    }
    const provenance = page
      .getByRole('link', { name: t('triageResult.provenance'), exact: true })
      .first();
    await provenance.focus();
    await page.keyboard.press('Enter');
    expect(new URL(page.url()).searchParams.get('diagnostic')).toBe('calls');
    expect(new URL(page.url()).searchParams.get('node_attempt_id')).toBeTruthy();
    const overflow = await page.evaluate(() => ({
      width: document.documentElement.scrollWidth - window.innerWidth,
      elements: [...document.querySelectorAll('main *')]
        .filter((element) => element.getBoundingClientRect().right > window.innerWidth + 1)
        .slice(-18)
        .map((element) => ({
          tag: element.tagName,
          className: element.className,
          text: element.textContent?.slice(0, 100),
          right: element.getBoundingClientRect().right,
        })),
    }));
    expect(overflow.width, JSON.stringify(overflow.elements)).toBeLessThanOrEqual(0);
    await page.reload();
    await expect(
      page.getByRole('heading', { name: t('triageResult.analyst'), exact: true }),
    ).toBeVisible();
  }
  if (storage && !resumed) {
    await page
      .getByRole('combobox', { name: translate('ru', 'theme.label'), exact: true })
      .press('Enter');
    await page
      .getByRole('option', { name: translate('ru', 'theme.dark'), exact: true })
      .press('Enter');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await page.context().storageState({ path: storage });
  }
  expect(writes).toEqual([]);
});
