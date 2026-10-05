// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  ResourceSearchApi,
  ResourceSearchKind,
  ResourceSearchPage,
} from '../../api/resourceSearch';
import { createConsoleQueryClient, RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS } from '../query';
import { PresentationProvider } from '../presentation/PresentationProvider';
import { PREFERENCES_KEY } from '../presentation/preferences';
import { CommandCenter } from './CommandCenter';
import type { ResourceSearchScope } from './searchScope';

const projectId = '11111111-1111-4111-8111-111111111111';
const buildId = '22222222-2222-4222-8222-222222222222';
const agentId = '33333333-3333-4333-8333-333333333333';
const poolId = '44444444-4444-4444-8444-444444444444';
const queryClients = new Set<ReturnType<typeof createConsoleQueryClient>>();

afterEach(() => {
  cleanup();
  for (const queryClient of queryClients) queryClient.clear();
  queryClients.clear();
  localStorage.clear();
  vi.useRealTimers();
});

describe('CommandCenter', () => {
  it('offers global navigation and presentation commands without state-changing actions', async () => {
    const api = searchApi(async () => emptyPage());
    const { router } = renderCenter(api);
    const search = screen.getByRole('searchbox', { name: 'Search query' });

    expect(document.activeElement).toBe(search);
    expect(screen.getByRole('heading', { name: 'Navigate' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Presentation commands' })).toBeTruthy();
    expect(screen.getByText('Esc')).toBeTruthy();
    expect(
      screen.queryByRole('button', { name: /cancel|drain|start build|release hold/i }),
    ).toBeNull();
    expect(screen.queryByRole('button', { name: /use (english|russian)/i })).toBeNull();
    expect(api.searchResources).not.toHaveBeenCalled();

    fireEvent.keyDown(search, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Open Projects' }));
    fireEvent.click(screen.getByRole('button', { name: 'Open Audit' }));

    expect(router.state.location.pathname).toBe('/audit');
    expect(screen.queryByRole('dialog', { name: 'Search resources' })).toBeNull();
  });

  it('debounces scoped searches and aborts the superseded request', async () => {
    vi.useFakeTimers();
    const calls: Array<{
      kinds: readonly ResourceSearchKind[];
      query: string;
      resolve: (page: ResourceSearchPage) => void;
      signal: AbortSignal | undefined;
    }> = [];
    const api = searchApi(
      (query, kinds, _cursor, signal) =>
        new Promise<ResourceSearchPage>((resolve) => calls.push({ kinds, query, resolve, signal })),
    );
    renderCenter(api, ['agent', 'agent_pool']);
    const search = screen.getByRole('searchbox', { name: 'Search query' });

    expect(screen.queryByRole('heading', { name: 'Navigate' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Presentation commands' })).toBeNull();
    fireEvent.change(search, { target: { value: 'alpha' } });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS - 1);
    expect(calls).toHaveLength(0);
    await advance(1);
    expect(calls[0]).toMatchObject({ kinds: ['agent', 'agent_pool'], query: 'alpha' });

    fireEvent.change(search, { target: { value: 'beta' } });
    expect(calls[0]?.signal?.aborted).toBe(true);
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);
    expect(calls).toHaveLength(2);

    await act(async () => {
      calls[1]?.resolve(emptyPage());
      await Promise.resolve();
    });
    await flushPromises();
    expect(screen.getByText('No visible resources match this query.')).toBeTruthy();
  });

  it('groups pages, toggles favorites, and records navigation as a recent identity', async () => {
    vi.useFakeTimers();
    const api = searchApi(async (_query, _kinds, cursor) =>
      cursor === null
        ? page(
            [
              result('project', projectId, 'Alpha Project', 'Root Project'),
              result('build', buildId, 'Alpha Build', 'Succeeded'),
            ],
            'next-page',
          )
        : page([result('agent', agentId, 'Alpha Agent', 'Ready')]),
    );
    const { router } = renderCenter(api);
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search query' }), {
      target: { value: 'alpha' },
    });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);

    expect(screen.getByRole('heading', { name: 'Projects' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Builds' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Add Alpha Build to favorites' }));
    expect(storedPreferences().favorites).toEqual([{ id: buildId, kind: 'build' }]);

    fireEvent.click(screen.getByRole('button', { name: 'Load more results' }));
    await flushPromises();
    expect(screen.getByRole('button', { name: 'Open Alpha Agent' })).toBeTruthy();
    expect(api.searchResources).toHaveBeenLastCalledWith(
      'alpha',
      [],
      'next-page',
      expect.any(AbortSignal),
    );

    fireEvent.click(screen.getByRole('button', { name: 'Open Alpha Build' }));
    expect(router.state.location.pathname).toBe(`/builds/${buildId}`);
    expect(storedPreferences().recents).toEqual([{ id: buildId, kind: 'build' }]);
  });

  it('rejects an oversized UTF-8 query locally and distinguishes a valid empty result', async () => {
    vi.useFakeTimers();
    const api = searchApi(async () => emptyPage());
    renderCenter(api, ['project']);
    const search = screen.getByRole('searchbox', { name: 'Search query' });

    fireEvent.change(search, { target: { value: 'я'.repeat(65) } });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);
    expect(screen.getByRole('alert').textContent).toContain('128');
    expect(api.searchResources).not.toHaveBeenCalled();

    fireEvent.change(search, { target: { value: 'missing' } });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);
    expect(screen.getByText('No visible resources match this query.')).toBeTruthy();
    expect(api.searchResources).toHaveBeenCalledWith(
      'missing',
      ['project'],
      null,
      expect.any(AbortSignal),
    );
  });

  it('presents safe initial failure details and permits a bounded retry', async () => {
    vi.useFakeTimers();
    const implementation = vi
      .fn<ResourceSearchApi['searchResources']>()
      .mockRejectedValueOnce(new Error('private failure'))
      .mockResolvedValueOnce(page([result('agent_pool', poolId, 'Primary Pool', null)]));
    const api: ResourceSearchApi = { searchResources: implementation };
    renderCenter(api);
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search query' }), {
      target: { value: 'primary' },
    });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);

    expect(screen.getByText('Resource search could not be loaded.')).toBeTruthy();
    expect(document.body.textContent).not.toContain('private failure');
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await flushPromises();
    expect(screen.getByRole('button', { name: 'Open Primary Pool' })).toBeTruthy();
  });

  it('keeps prior results visible when loading the next page fails', async () => {
    vi.useFakeTimers();
    const api = searchApi(async (_query, _kinds, cursor) => {
      if (cursor !== null) throw new Error('private next-page failure');
      return page([result('project', projectId, 'Retained Project', null)], 'next-page');
    });
    renderCenter(api);
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search query' }), {
      target: { value: 'retained' },
    });
    await advance(RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS);
    expect(screen.getByRole('button', { name: 'Open Retained Project' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Load more results' }));
    await flushPromises();
    expect(
      screen.getByText('Refresh failed. Showing the last loaded Search results.'),
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Open Retained Project' })).toBeTruthy();
    expect(document.body.textContent).not.toContain('private next-page failure');
  });
});

function renderCenter(api: ResourceSearchApi, scope: ResourceSearchScope = []) {
  const queryClient = createConsoleQueryClient();
  queryClients.add(queryClient);
  const router = createMemoryRouter(
    [
      {
        path: '*',
        element: (
          <PresentationProvider>
            <QueryClientProvider client={queryClient}>
              <Harness api={api} scope={scope} />
            </QueryClientProvider>
          </PresentationProvider>
        ),
      },
    ],
    { initialEntries: ['/projects'] },
  );
  render(<RouterProvider router={router} />);
  return { queryClient, router };
}

function Harness({ api, scope }: { api: ResourceSearchApi; scope: ResourceSearchScope }) {
  const [open, setOpen] = useState(true);
  return open ? (
    <CommandCenter
      api={api}
      onClose={() => setOpen(false)}
      returnFocus={document.body}
      scope={scope}
    />
  ) : (
    <p>Command center closed</p>
  );
}

function searchApi(implementation: ResourceSearchApi['searchResources']): ResourceSearchApi {
  return { searchResources: vi.fn(implementation) };
}

function emptyPage(): ResourceSearchPage {
  return page([]);
}

function page(
  items: ResourceSearchPage['items'],
  nextCursor: string | null = null,
): ResourceSearchPage {
  return { items, next_cursor: nextCursor };
}

function result(
  kind: ResourceSearchKind,
  id: string,
  label: string,
  context: string | null,
): ResourceSearchPage['items'][number] {
  return { context, id, kind, label };
}

async function advance(milliseconds: number): Promise<void> {
  await act(async () => {
    vi.advanceTimersByTime(milliseconds);
    await Promise.resolve();
  });
  await act(async () => {
    await Promise.resolve();
    vi.advanceTimersByTime(0);
    await Promise.resolve();
  });
}

async function flushPromises(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
  await act(async () => {
    vi.advanceTimersByTime(0);
    await Promise.resolve();
  });
}

function storedPreferences(): Record<string, unknown> {
  return JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? '{}') as Record<string, unknown>;
}
