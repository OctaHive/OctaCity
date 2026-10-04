// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createConsoleQueryClient } from '../../app/query';
import { BuildLogSearch } from './BuildLogSearch';
import type { BuildLogSearchApi, BuildLogSearchPage } from './api';

const PROJECT_ID = '11111111-1111-4111-8111-111111111111';
const BUILD_ID = '22222222-2222-4222-8222-222222222222';
const ATTEMPT_ID = '33333333-3333-4333-8333-333333333333';
const JOB_ONE = '44444444-4444-4444-8444-444444444444';
const JOB_TWO = '55555555-5555-4555-8555-555555555555';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('Build log search', () => {
  it('follows literal cursor pages, warns on delayed projection, and renders snippets as text', async () => {
    const consoleLog = vi.spyOn(console, 'log').mockImplementation(() => undefined);
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const first = page(
      [hit('chunk-1', 'unsafe <script>window.stolen = true</script> snippet')],
      'next-page',
      false,
    );
    const second = page([hit('chunk-2', 'second redacted match')], null, true);
    const api: BuildLogSearchApi = {
      searchBuildLogs: vi.fn().mockResolvedValueOnce(first).mockResolvedValueOnce(second),
    };
    renderSearch(
      `?log_query=needle&log_mode=literal&log_scope=job%3A${JOB_TWO}&log_stream=stderr`,
      api,
    );

    const results = await screen.findByRole('region', { name: 'Redacted log search' });
    expect(await within(results).findByText(/unsafe <script>window\.stolen/)).toBeTruthy();
    expect(document.querySelector('script')).toBeNull();
    expect(within(results).getByRole('status').textContent).toBe(
      'Results may be incomplete. Indexed through 17; committed through 19.',
    );
    expect(api.searchBuildLogs).toHaveBeenNthCalledWith(
      1,
      PROJECT_ID,
      {
        attemptId: ATTEMPT_ID,
        buildId: BUILD_ID,
        jobId: JOB_TWO,
        mode: 'literal',
        query: 'needle',
        stream: 'stderr',
      },
      null,
      expect.any(AbortSignal),
    );

    await userEvent.click(within(results).getByRole('button', { name: 'Load more log matches' }));
    await screen.findByText('second redacted match');
    expect(api.searchBuildLogs).toHaveBeenNthCalledWith(
      2,
      PROJECT_ID,
      expect.objectContaining({ mode: 'literal', query: 'needle' }),
      'next-page',
      expect.any(AbortSignal),
    );
    expect(consoleLog).not.toHaveBeenCalled();
    expect(consoleWarn).not.toHaveBeenCalled();
  });

  it('stores full-text filters in the URL and distinguishes an authoritative empty result', async () => {
    const api: BuildLogSearchApi = {
      searchBuildLogs: vi.fn().mockResolvedValue(page([], null, true)),
    };
    const { router } = renderSearch('', api);

    expect(api.searchBuildLogs).not.toHaveBeenCalled();
    await userEvent.type(screen.getByRole('searchbox', { name: 'Log query' }), 'compile failed');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Search mode' }),
      'full_text',
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Search scope' }),
      'attempt',
    );
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Log stream' }), 'stdout');
    await userEvent.click(screen.getByRole('button', { name: 'Search logs' }));

    await waitFor(() =>
      expect(router.state.location.search).toBe(
        '?log_query=compile+failed&log_mode=full_text&log_scope=attempt&log_stream=stdout',
      ),
    );
    expect(await screen.findByText('No redacted log matches the selected filters.')).toBeTruthy();
    expect(api.searchBuildLogs).toHaveBeenCalledWith(
      PROJECT_ID,
      {
        attemptId: ATTEMPT_ID,
        buildId: BUILD_ID,
        jobId: null,
        mode: 'full_text',
        query: 'compile failed',
        stream: 'stdout',
      },
      null,
      expect.any(AbortSignal),
    );
  });

  it('rejects an unavailable Job scope without widening the search to the Build', async () => {
    const api: BuildLogSearchApi = {
      searchBuildLogs: vi.fn(),
    };
    renderSearch('?log_query=needle&log_scope=job%3Aremoved-job', api);

    expect(
      await screen.findByText(
        'The Job selected by this log-search URL is not available in the current Attempt.',
      ),
    ).toBeTruthy();
    expect(api.searchBuildLogs).not.toHaveBeenCalled();
  });

  it('retains and marks previous matches when a refresh fails', async () => {
    const api: BuildLogSearchApi = {
      searchBuildLogs: vi
        .fn()
        .mockResolvedValueOnce(page([hit('chunk-1', 'retained match')], null, true))
        .mockRejectedValueOnce(new Error('refresh failed')),
    };
    renderSearch('?log_query=needle&log_mode=literal', api);

    expect(await screen.findByText('retained match')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Search logs' }));

    expect(
      await screen.findByText('Refresh failed. Showing the last loaded redacted log search data.'),
    ).toBeTruthy();
    expect(screen.getByText('retained match')).toBeTruthy();
  });
});

function renderSearch(search: string, api: BuildLogSearchApi) {
  const router = createMemoryRouter(
    [
      {
        path: '/builds/:buildId',
        element: (
          <BuildLogSearch
            api={api}
            attemptId={ATTEMPT_ID}
            buildId={BUILD_ID}
            jobs={[
              { id: JOB_ONE, pipeline_node_id: 'prepare' },
              { id: JOB_TWO, pipeline_node_id: 'compile' },
            ]}
            projectId={PROJECT_ID}
          />
        ),
      },
    ],
    { initialEntries: [`/builds/${BUILD_ID}${search}`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router };
}

function page(
  items: BuildLogSearchPage['items'],
  nextCursor: string | null,
  caughtUp: boolean,
): BuildLogSearchPage {
  return {
    freshness: {
      caught_up: caughtUp,
      committed_through: 19,
      indexed_through: caughtUp ? 19 : 17,
    },
    items,
    next_cursor: nextCursor,
  };
}

function hit(chunkId: string, snippet: string): BuildLogSearchPage['items'][number] {
  return {
    attempt_id: ATTEMPT_ID,
    build_id: BUILD_ID,
    chunk_id: chunkId,
    first_sequence: 10,
    job_id: JOB_TWO,
    last_sequence: 12,
    occurred_at_unix_ms: 1_700_000_000_000,
    snippet,
    stream: 'stderr',
  };
}
