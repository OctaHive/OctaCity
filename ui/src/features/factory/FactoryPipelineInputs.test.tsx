// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factorySourcesApi } from './sourceApi';
import { FactoryPipelineInputs } from './FactoryPipelineInputs';

afterEach(cleanup);
it('keeps manual Artifact intake available without loading integration choices or creating an internal task', async () => {
  const api = { ...factorySourcesApi, connections: vi.fn(), list: vi.fn(), choices: vi.fn() };
  const onManual = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: (
              <FactoryPipelineInputs
                configuration={configurationFixture()}
                api={api}
                onManual={onManual}
              />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  expect(
    screen.getByText(
      'Manual intake requires no connection. Automatic intake is separate from task import, refresh and explicit task-card execution.',
    ),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Admit manual Work' }));
  expect(onManual).toHaveBeenCalledOnce();
  expect(api.connections).not.toHaveBeenCalled();
  expect(api.choices).not.toHaveBeenCalled();
  expect(api.list).not.toHaveBeenCalled();
  expect(screen.queryByLabelText('Credential value')).toBeNull();
});

it('guides missing automatic-source setup to Integrations while preserving manual intake', async () => {
  const api = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [], next_cursor: null })),
    list: vi.fn(async () => ({ items: [], next_cursor: null })),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: (
              <FactoryPipelineInputs
                configuration={configurationFixture()}
                api={api}
                onManual={() => {}}
              />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  expect(
    await screen.findByText(
      'No usable source is configured. Set up a connection and Project access in Integrations.',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Manage integrations' }).getAttribute('href')).toBe(
    '/integrations',
  );
  expect(screen.getByRole('button', { name: 'Admit manual Work' })).toHaveProperty(
    'disabled',
    false,
  );
});
