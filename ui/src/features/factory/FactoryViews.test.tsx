// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { PresentationProvider } from '../../app/presentation/PresentationProvider';
import { FactoryLandingView, FactoryRunView } from './FactoryViews';
import { factoryApi } from './api';
import { configurationFixture, runFixture } from '../../../tests/fixtures/factory';
afterEach(cleanup);
it('opens the selected Project workbench from a copied link and exposes visible configuration versions', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const api = {
    ...factoryApi,
    listRuns: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          ...configuration.configuration.reference,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => configuration),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  const router = createMemoryRouter([{ path: '*', element: <FactoryLandingView api={api} /> }], {
    initialEntries: [`/factory?project_id=${projectId}&state=waiting`],
  });
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  expect(await screen.findByText('manual.medium · v1')).toBeTruthy();
  expect(api.listRuns).toHaveBeenCalledWith(
    projectId,
    { state: 'waiting' },
    null,
    expect.any(AbortSignal),
  );
});
it('favorites an exact Run identity without persisting its task or displayed diagnostics', async () => {
  const run = runFixture();
  const api = { ...factoryApi, getRun: vi.fn(async () => run) };
  let stored = '';
  const storage = {
    getItem: () => null,
    setItem: (_key: string, value: string) => {
      stored = value;
    },
  };
  const router = createMemoryRouter(
    [{ path: '/factory/:factoryRunId', element: <FactoryRunView api={api} /> }],
    { initialEntries: [`/factory/${run.id}?state=waiting&source=manual`] },
  );
  render(
    <PresentationProvider storage={storage}>
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </PresentationProvider>,
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Add to Favorites' }));
  expect(JSON.parse(stored).favorites).toEqual([{ kind: 'factory_run', id: run.id }]);
  expect(stored).not.toContain(run.external_identity);
  const ownerLink = screen.getByRole('link', { name: 'Open Project Factory workbench' });
  const ownerUrl = new URL(ownerLink.getAttribute('href') ?? '', 'http://localhost');
  expect(ownerUrl.searchParams.get('state')).toBe('waiting');
  expect(ownerUrl.searchParams.get('source')).toBe('manual');
  expect(screen.getByRole('button', { name: 'Remove from Favorites' })).toBeTruthy();
});
