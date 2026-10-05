import { expect, test, type Page } from '@playwright/test';

import { installOperatorFixture } from './operatorFixture';
import { PRIMARY_ROUTES } from './primaryRoutes';

for (const route of PRIMARY_ROUTES) {
  test(`${route.path} has labeled semantics, visible keyboard focus, and a usable narrow layout`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.setViewportSize({ height: 900, width: 1_440 });
    await page.goto(route.path);
    await expect(page.getByRole('status', { name: 'Server ready' })).toBeVisible();
    await expect(
      page.getByRole('main').getByRole('heading', { exact: true, level: 1, name: route.heading }),
    ).toBeVisible();

    expect(await accessibilityViolations(page)).toEqual([]);
    await expectVisibleKeyboardFocus(page);

    await page.setViewportSize({ height: 900, width: 640 });
    await expect(page.getByRole('main')).toBeVisible();
    await expect(page.getByRole('button', { name: /Open .* explorer/u })).toBeVisible();
    expect(await viewportOverflow(page)).toBe(0);
    expect(await accessibilityViolations(page)).toEqual([]);
    await expectVisibleKeyboardFocus(page);
  });
}

test('semantic checks reject descendant text as a structural accessible name', async ({ page }) => {
  await page.setContent(`
    <html lang="en">
      <head><title>Invalid semantics</title></head>
      <body>
        <main><h1>Fixture</h1><table><tr><th>Visible content</th></tr></table></main>
        <div role="dialog">Visible dialog content</div>
        <div role="img">Visible graphic content</div>
      </body>
    </html>
  `);

  expect(await accessibilityViolations(page)).toEqual(
    expect.arrayContaining([
      expect.stringContaining('unlabeled table'),
      expect.stringContaining('unlabeled dialog'),
      expect.stringContaining('unlabeled graphic'),
    ]),
  );
});

async function expectVisibleKeyboardFocus(page: Page) {
  await page.evaluate(() => {
    document.body.tabIndex = -1;
    document.body.focus();
  });
  const visited = new Set<string>();
  let completed = false;
  for (let index = 0; index < 200; index += 1) {
    await page.keyboard.press('Tab');
    const focus = await page.evaluate(() => {
      const active = document.activeElement;
      if (!(active instanceof HTMLElement)) return null;
      const style = getComputedStyle(active);
      const segments = [];
      let element: HTMLElement | null = active;
      while (element !== null && element !== document.body) {
        const parent: HTMLElement | null = element.parentElement;
        const position = parent === null ? 0 : Array.from(parent.children).indexOf(element);
        segments.push(`${element.tagName.toLowerCase()}:${position}`);
        element = parent;
      }
      return {
        element: `${active.tagName.toLowerCase()}${active.getAttribute('aria-label') === null ? '' : `[${active.getAttribute('aria-label')}]`}`,
        eligible:
          active.tabIndex >= 0 &&
          !('disabled' in active && active.disabled === true) &&
          active.getClientRects().length > 0 &&
          style.display !== 'none' &&
          style.visibility !== 'hidden' &&
          active.closest('[aria-hidden="true"]') === null,
        marker: segments.reverse().join('/'),
        visible:
          (style.outlineStyle !== 'none' && Number.parseFloat(style.outlineWidth) > 0) ||
          style.boxShadow !== 'none',
      };
    });
    expect(focus, `tab stop ${index + 1} must exist`).not.toBeNull();
    if (focus?.element === 'body') {
      completed = true;
      break;
    }
    expect(focus?.eligible, `tab stop ${index + 1} must be visible and operable`).toBe(true);
    if (!visited.has(focus?.marker ?? '')) {
      expect(
        focus?.visible,
        `${focus?.element ?? 'unknown'} (${focus?.marker ?? 'unmarked'}) must have a visible focus indicator`,
      ).toBe(true);
    }
    // Date/time inputs contain several browser-owned sub-controls while the same host remains active.
    visited.add(focus?.marker ?? '');
  }
  expect(completed, 'keyboard traversal must leave the document after its final tab stop').toBe(
    true,
  );
  expect(
    visited.size,
    'keyboard traversal must cover more than the former six-stop sample',
  ).toBeGreaterThan(6);
  await page.evaluate(() => {
    document.body.removeAttribute('tabindex');
  });
}

async function viewportOverflow(page: Page): Promise<number> {
  return page.evaluate(() =>
    Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
  );
}

async function accessibilityViolations(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const violations: string[] = [];
    const visible = (element: Element) => {
      const style = getComputedStyle(element);
      return (
        element.getClientRects().length > 0 &&
        style.display !== 'none' &&
        style.visibility !== 'hidden' &&
        element.closest('[aria-hidden="true"]') === null
      );
    };
    const explicitLabel = (element: Element) => {
      const direct = element.getAttribute('aria-label')?.trim();
      if (direct !== undefined && direct !== '') return direct;
      const labelledBy = element.getAttribute('aria-labelledby');
      if (labelledBy !== null) {
        const text = labelledBy
          .split(/\s+/u)
          .map((id) => document.getElementById(id)?.textContent?.trim() ?? '')
          .join(' ')
          .trim();
        if (text !== '') return text;
      }
      return element.getAttribute('title')?.trim() ?? '';
    };
    const controlLabel = (element: Element) => {
      const explicit = explicitLabel(element);
      if (explicit !== '') return explicit;
      if (
        element instanceof HTMLInputElement ||
        element instanceof HTMLSelectElement ||
        element instanceof HTMLTextAreaElement
      ) {
        const labels = Array.from(element.labels ?? [])
          .map((label) => label.textContent?.trim() ?? '')
          .join(' ');
        if (labels.trim() !== '') return labels;
      }
      if (
        element.matches('a[href], button, summary, [role="button"], [role="link"]') &&
        element.textContent?.trim()
      ) {
        return element.textContent.trim();
      }
      return '';
    };

    const ids = new Set<string>();
    for (const element of document.querySelectorAll('[id]')) {
      const id = element.id;
      if (ids.has(id)) violations.push(`duplicate id: ${id}`);
      ids.add(id);
    }

    for (const element of document.querySelectorAll('[aria-labelledby]')) {
      for (const id of element.getAttribute('aria-labelledby')?.split(/\s+/u) ?? []) {
        if (id !== '' && document.getElementById(id) === null) {
          violations.push(`missing aria-labelledby target: ${id}`);
        }
      }
    }

    const controls = document.querySelectorAll(
      'a[href], button, input:not([type="hidden"]), select, textarea, summary, [role="button"], [role="link"], [role="searchbox"], [role="separator"][tabindex]',
    );
    for (const control of controls) {
      if (visible(control) && controlLabel(control) === '') {
        violations.push(`unlabeled control: ${control.outerHTML.slice(0, 120)}`);
      }
    }

    for (const graphic of document.querySelectorAll('img, [role="img"]')) {
      const name =
        graphic instanceof HTMLImageElement ? graphic.alt.trim() : explicitLabel(graphic);
      if (visible(graphic) && name === '') {
        violations.push(`unlabeled graphic: ${graphic.outerHTML.slice(0, 120)}`);
      }
    }
    for (const dialog of document.querySelectorAll('[role="dialog"]')) {
      if (visible(dialog) && explicitLabel(dialog) === '') {
        violations.push(`unlabeled dialog: ${dialog.outerHTML.slice(0, 120)}`);
      }
    }
    for (const table of document.querySelectorAll('table')) {
      const caption = table.querySelector('caption')?.textContent?.trim() ?? '';
      if (visible(table) && explicitLabel(table) === '' && caption === '') {
        violations.push('unlabeled table');
      }
      if (visible(table) && table.querySelector('th') === null) {
        violations.push('table has no headers');
      }
    }

    if (document.querySelectorAll('main').length !== 1) {
      violations.push('document must contain exactly one main landmark');
    }
    if (document.querySelector('main h1') === null) {
      violations.push('main landmark must contain a level-one heading');
    }
    if (document.documentElement.lang.trim() === '') {
      violations.push('document language is missing');
    }
    if (document.title.trim() === '') {
      violations.push('document title is missing');
    }
    return violations;
  });
}
