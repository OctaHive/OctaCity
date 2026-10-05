// @vitest-environment jsdom

import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { formatTimestamp } from '../../shared/display';
import { PREFERENCES_KEY, type PreferenceStorage } from './preferences';
import { PresentationProvider, usePresentation } from './PresentationProvider';

afterEach(() => {
  cleanup();
  document.documentElement.lang = '';
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.resolvedTheme;
  vi.unstubAllGlobals();
});

describe('PresentationProvider', () => {
  it('persists language and updates document presentation immediately', async () => {
    const user = userEvent.setup();
    const storage = new MemoryStorage();
    render(
      <PresentationProvider storage={storage}>
        <Controls />
      </PresentationProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Russian' }));

    expect(document.documentElement.lang).toBe('ru');
    expect(document.title).toBe('Консоль оператора OctaCity');
    expect(screen.getByText('Проекты')).toBeTruthy();
    expect(screen.getByTestId('localized-time').textContent).toBe(
      formatTimestamp(Date.UTC(2026, 0, 5, 12, 34, 56), 'ru-RU').display,
    );
    expect(JSON.parse(storage.record ?? '').language).toBe('ru');
  });

  it('follows operating-system theme changes only in system mode', async () => {
    const user = userEvent.setup();
    const media = new MutableMediaQuery(false);
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => media as MediaQueryList),
    );
    render(
      <PresentationProvider storage={new MemoryStorage()}>
        <Controls />
      </PresentationProvider>,
    );

    expect(document.documentElement.dataset.resolvedTheme).toBe('light');
    media.change(true);
    expect(document.documentElement.dataset.resolvedTheme).toBe('dark');

    await user.click(screen.getByRole('button', { name: 'Light theme' }));
    media.change(false);
    media.change(true);
    expect(document.documentElement.dataset.resolvedTheme).toBe('light');
  });

  it('stores only the local notification-center timestamp', async () => {
    const user = userEvent.setup();
    const storage = new MemoryStorage();
    render(
      <PresentationProvider storage={storage}>
        <Controls />
      </PresentationProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Open notifications' }));

    const record = storage.record ?? '';
    expect(JSON.parse(record).notificationLastOpenedAt).toEqual(expect.any(Number));
    expect(record).not.toMatch(/items|payload|request|credential|download|logs/i);
  });

  it('persists bounded resource identities without resource labels or payloads', async () => {
    const user = userEvent.setup();
    const storage = new MemoryStorage();
    render(
      <PresentationProvider storage={storage}>
        <Controls />
      </PresentationProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Remember resource' }));

    const preferences = JSON.parse(storage.record ?? '') as Record<string, unknown>;
    const identity = { id: '11111111-1111-4111-8111-111111111111', kind: 'project' };
    expect(preferences).toMatchObject({
      expanded: [identity],
      favorites: [identity],
      recents: [identity],
    });
    expect(storage.record).not.toMatch(/label|name|payload/i);
  });
});

function Controls() {
  const {
    addRecent,
    markNotificationsOpened,
    setExpanded,
    setLanguage,
    setTheme,
    t,
    locale,
    toggleFavorite,
  } = usePresentation();
  const identity = { id: '11111111-1111-4111-8111-111111111111', kind: 'project' } as const;
  return (
    <>
      <span>{t('section.projects')}</span>
      <time data-testid="localized-time">
        {formatTimestamp(Date.UTC(2026, 0, 5, 12, 34, 56), locale).display}
      </time>
      <button onClick={() => setLanguage('ru')} type="button">
        Russian
      </button>
      <button onClick={() => setTheme('light')} type="button">
        Light theme
      </button>
      <button onClick={markNotificationsOpened} type="button">
        Open notifications
      </button>
      <button
        onClick={() => {
          setExpanded(identity, true);
          addRecent(identity);
          toggleFavorite(identity);
        }}
        type="button"
      >
        Remember resource
      </button>
    </>
  );
}

class MemoryStorage implements PreferenceStorage {
  record: string | null = null;

  getItem(key: string): string | null {
    expect(key).toBe(PREFERENCES_KEY);
    return this.record;
  }

  setItem(key: string, value: string): void {
    expect(key).toBe(PREFERENCES_KEY);
    this.record = value;
  }
}

class MutableMediaQuery {
  matches: boolean;
  readonly media = '(prefers-color-scheme: dark)';
  onchange: ((this: MediaQueryList, event: MediaQueryListEvent) => unknown) | null = null;
  private readonly listeners = new Set<(event: MediaQueryListEvent) => void>();

  constructor(matches: boolean) {
    this.matches = matches;
  }

  addEventListener(_type: 'change', listener: (event: MediaQueryListEvent) => void): void {
    this.listeners.add(listener);
  }

  removeEventListener(_type: 'change', listener: (event: MediaQueryListEvent) => void): void {
    this.listeners.delete(listener);
  }

  change(matches: boolean): void {
    this.matches = matches;
    const event = { matches } as MediaQueryListEvent;
    act(() => {
      for (const listener of this.listeners) listener(event);
    });
  }

  addListener(): void {}
  dispatchEvent(): boolean {
    return true;
  }
  removeListener(): void {}
}
