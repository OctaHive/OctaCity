// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryPools } from './FactoryPools';
import { configurationFixture } from '../../../tests/fixtures/factory';

afterEach(cleanup);

it('selects server-owned named phase-ready policies and resets continuation on a different pool', async () => {
  const configuration = configurationFixture();
  configuration.pool_policy_digests = {
    'operator-first-queue': 'a'.repeat(64),
    'operator-second-queue': 'b'.repeat(64),
  };
  const api = {
    listPhaseReady: vi.fn(async () => ({ items: [], next_cursor: 'pool-next', total: 0 })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryPools api={api} configuration={configuration} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('No phase-ready Work.')).toBeTruthy();
  expect(api.listPhaseReady).toHaveBeenLastCalledWith(
    configuration.configuration.reference.project_id,
    'a'.repeat(64),
    null,
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Load more phase-ready Work' }));
  expect(api.listPhaseReady).toHaveBeenLastCalledWith(
    configuration.configuration.reference.project_id,
    'a'.repeat(64),
    'pool-next',
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Phase-ready pool' }));
  await userEvent.click(screen.getByRole('option', { name: 'operator-second-queue' }));
  expect(router.state.location.search).toBe('?policy_digest=' + 'b'.repeat(64));
  expect(api.listPhaseReady).toHaveBeenLastCalledWith(
    configuration.configuration.reference.project_id,
    'b'.repeat(64),
    null,
    expect.any(AbortSignal),
  );
});
