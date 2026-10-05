// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Bell } from 'lucide-react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  OperatorAttentionApi,
  OperatorAttentionItem,
  OperatorAttentionScope,
} from '../../api/operatorAttention';
import { NotificationProvider, useNotifications } from '../notifications/NotificationProvider';
import { PresentationProvider, usePresentation } from '../presentation/PresentationProvider';
import { createConsoleQueryClient } from '../query';
import {
  DEFAULT_PREFERENCES,
  PREFERENCES_KEY,
  type PreferenceStorage,
} from '../presentation/preferences';
import { NotificationCenter } from './NotificationCenter';

const FAVORITE_BUILD = '11111111-1111-4111-8111-111111111111';
const FAVORITE_AGENT = '22222222-2222-4222-8222-222222222222';
const UNRELATED_BUILD = '99999999-9999-4999-8999-999999999999';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('NotificationCenter', () => {
  it('shows only session, favorite, and critical items with local seen and reload semantics', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date('2026-10-05T12:00:00Z'));
    const storage = preferenceStorage([{ id: FAVORITE_BUILD, kind: 'build' }]);
    const api = attentionApi([
      attentionItem('favorite-event', 'build', FAVORITE_BUILD, 1_700_000_000_000),
      attentionItem('unrelated-event', 'build', UNRELATED_BUILD, 1_700_000_000_100),
      criticalItem('critical-active', 1_700_000_000_200, null),
      criticalItem('critical-resolved', 1_700_000_000_300, 1_700_000_000_400),
    ]);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const first = renderCenter(api, storage, true);

    await user.click(screen.getByRole('button', { name: 'Report command success' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Notifications, 4 unseen' })).toBeTruthy(),
    );
    await user.click(screen.getByRole('button', { name: 'Notifications, 4 unseen' }));

    const dialog = screen.getByRole('dialog', { name: 'Notification center' });
    expect(within(dialog).getByText('Current command')).toBeTruthy();
    expect(screen.getByText('favorite-event summary')).toBeTruthy();
    expect(screen.getByText('critical-active summary')).toBeTruthy();
    expect(screen.getByText('critical-resolved summary')).toBeTruthy();
    expect(screen.queryByText('unrelated-event summary')).toBeNull();
    expect(screen.getByText('Resolved')).toBeTruthy();
    expect(screen.getByRole('link', { name: /favorite-event summary/u }).getAttribute('href')).toBe(
      `/builds/${FAVORITE_BUILD}`,
    );
    expect(document.activeElement).toBe(
      screen.getByRole('button', { name: 'Close notification center' }),
    );
    expect(JSON.parse(storage.getItem(PREFERENCES_KEY) ?? '{}')).toMatchObject({
      notificationLastOpenedAt: expect.any(Number),
    });
    expect(storage.getItem(PREFERENCES_KEY)).not.toMatch(
      /favorite-event|critical-active|Current command|request[_-]?id/iu,
    );

    await user.keyboard('{Escape}');
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Notifications' }));
    first.unmount();

    renderCenter(api, storage, false);
    expect(await screen.findByRole('button', { name: 'Notifications' })).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Notifications' }));
    expect(screen.queryByText('Current command')).toBeNull();
    expect(await screen.findByText('favorite-event summary')).toBeTruthy();
  });

  it('rebinds the bounded scope when favorites change and paginates explicitly', async () => {
    const calls: { cursor: string | null; scope: OperatorAttentionScope }[] = [];
    const api: OperatorAttentionApi = {
      async listOperatorAttention(scope, cursor) {
        calls.push({ cursor, scope });
        if (cursor === 'next') {
          return {
            items: [criticalItem('older-critical', 1_600_000_000_000, null)],
            next_cursor: null,
          };
        }
        return {
          items:
            scope.agentIds.length === 0
              ? []
              : [attentionItem('favorite-agent', 'agent', FAVORITE_AGENT, 1_700_000_000_000)],
          next_cursor: scope.agentIds.length === 0 ? null : 'next',
        };
      },
    };
    const user = userEvent.setup();
    renderCenter(api, preferenceStorage([]), false, true);

    await waitFor(() => expect(calls.at(-1)?.scope.agentIds).toEqual([]));
    await user.click(screen.getByRole('button', { name: 'Toggle Agent favorite' }));
    await waitFor(() => expect(calls.at(-1)?.scope.agentIds).toEqual([FAVORITE_AGENT]));
    expect(calls.at(-1)?.scope.includeCriticalConditions).toBe(true);

    await user.click(screen.getByRole('button', { name: /Notifications/u }));
    expect(await screen.findByText('favorite-agent summary')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Load older notifications' }));
    expect(await screen.findByText('older-critical summary')).toBeTruthy();
    expect(calls.at(-1)?.cursor).toBe('next');
  });
});

function renderCenter(
  api: OperatorAttentionApi,
  storage: PreferenceStorage,
  reportAction: boolean,
  toggleFavorite = false,
) {
  const queryClient = createConsoleQueryClient();
  return render(
    <PresentationProvider storage={storage}>
      <QueryClientProvider client={queryClient}>
        <NotificationProvider>
          <MemoryRouter>
            {reportAction ? <ActionReporter /> : null}
            {toggleFavorite ? <FavoriteToggle /> : null}
            <NotificationCenter
              api={api}
              icon={<Bell aria-hidden="true" />}
              triggerClassName="notification-trigger"
            />
          </MemoryRouter>
        </NotificationProvider>
      </QueryClientProvider>
    </PresentationProvider>,
  );
}

function ActionReporter() {
  const { reportCommandOutcome } = useNotifications();
  return (
    <button
      onClick={() => reportCommandOutcome(Symbol(), 'Current command', 'success')}
      type="button"
    >
      Report command success
    </button>
  );
}

function FavoriteToggle() {
  const { toggleFavorite } = usePresentation();
  return (
    <button onClick={() => toggleFavorite({ id: FAVORITE_AGENT, kind: 'agent' })} type="button">
      Toggle Agent favorite
    </button>
  );
}

function attentionApi(items: readonly OperatorAttentionItem[]): OperatorAttentionApi {
  return {
    async listOperatorAttention() {
      return { items: [...items], next_cursor: null };
    },
  };
}

function attentionItem(
  id: string,
  kind: 'agent' | 'agent_pool' | 'build',
  targetId: string,
  occurredAt: number,
): OperatorAttentionItem {
  return {
    category: kind,
    code: `${kind}_attention`,
    id,
    occurred_at_unix_ms: occurredAt,
    resolved_at_unix_ms: null,
    severity: 'warning',
    summary: `${id} summary`,
    target: { id: targetId, kind },
  };
}

function criticalItem(
  id: string,
  occurredAt: number,
  resolvedAt: number | null,
): OperatorAttentionItem {
  return {
    category: 'critical_system',
    code: 'required_dependency_unavailable',
    id,
    occurred_at_unix_ms: occurredAt,
    resolved_at_unix_ms: resolvedAt,
    severity: 'critical',
    summary: `${id} summary`,
    target: null,
  };
}

function preferenceStorage(favorites: readonly { id: string; kind: 'build' }[]): PreferenceStorage {
  const values = new Map<string, string>([
    [PREFERENCES_KEY, JSON.stringify({ ...DEFAULT_PREFERENCES, favorites })],
  ]);
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
  };
}
