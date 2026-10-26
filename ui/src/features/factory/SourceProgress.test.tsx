// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { AutomaticInputs } from './AutomaticInputs';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';
import { ManagementApiError } from '../../api/client';

afterEach(cleanup);
function fixture(data = integrationFixture('WorkSourceDiagnostics')) {
  const configuration = configurationFixture();
  if (data.configuration.kind === 'input')
    data.configuration.definition.definition.target = configuration.configuration.reference;
  const diagnostics = vi.fn<FactorySourcesApi['diagnostics']>(async () => data);
  const api = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [], next_cursor: null })),
    list: vi.fn(async () => ({ items: [data.configuration], next_cursor: null })),
    diagnostics,
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          { path: '*', element: <AutomaticInputs configuration={configuration} api={api} /> },
        ])}
      />
    </QueryClientProvider>,
  );
  return { data, diagnostics };
}

it('shows retained Work/Run correlations and separates pending from applied reporting without requiring a card', async () => {
  const { data, diagnostics } = fixture();
  const first = data.admissions[0]!;
  first.reporting.push({ ...first.reporting[0]!, operation_id: '44'.repeat(32), state: 'applied' });
  expect(diagnostics).not.toHaveBeenCalled();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  const panel = await screen.findByRole('region', { name: 'Retained source progress' });
  expect(within(panel).getByText('Awaiting external confirmation')).toBeTruthy();
  expect(within(panel).getByText('External effects confirmed')).toBeTruthy();
  expect(within(panel).getAllByText('Projection attempt 0')).toHaveLength(2);
  expect(within(panel).getByText('Eligible dispatch time')).toBeTruthy();
  expect(
    within(panel).getByText(
      'Reporting health does not determine task business status or Run execution.',
    ),
  ).toBeTruthy();
  expect(within(panel).getByRole('link', { name: 'Open Run' }).getAttribute('href')).toBe(
    `/factory/${first.association.run_id}`,
  );
  expect(within(panel).getByRole('link', { name: 'Work summary' }).getAttribute('href')).toBe(
    `/factory/${first.association.run_id}?diagnostic=work&diagnostic_id=${first.association.work_id}`,
  );
  expect(panel.textContent).toContain(first.association.native_correlation);
  expect(within(panel).queryByRole('link', { name: 'Open task card' })).toBeNull();
  expect(diagnostics.mock.calls[0]!.slice(0, 3)).toEqual([
    data.configuration.definition.project_id,
    {
      kind: 'input',
      id: data.configuration.definition.id,
      version: data.configuration.definition.version,
    },
    null,
  ]);
});

it('identifies ambiguous grouped selection and deferred scans without claiming admission or an empty successful read', async () => {
  const data = integrationFixture('WorkSourceDiagnostics');
  if (data.configuration.kind !== 'input') throw new Error('Expected canonical input');
  const input = data.configuration.definition;
  data.configuration = {
    kind: 'group',
    definition: { id: 'group', project_id: input.project_id, version: 2, members: [input] },
  };
  data.scan = {
    started_at: 123,
    window: [10, 20],
    reconciliation: true,
    selection: {
      item: { resource: 'native-container', item: 'native-task' },
      matching_inputs: ['first', 'second'],
    },
    schedule: { attempts: 2, failure: 'rate_limited', retry_at: 456, exhausted: false },
  };
  data.admissions = [];
  const { diagnostics } = fixture(data);
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  const panel = await screen.findByRole('region', { name: 'Retained source progress' });
  expect(
    await within(panel).findByText('Ambiguous: more than one exclusive input matches'),
  ).toBeTruthy();
  expect(panel.textContent).toContain('first, second');
  expect(panel.textContent).toContain('Rate limited');
  expect(
    within(panel).getByText(
      'No visible admitted Work in this page; this does not prove that no earlier or hidden execution exists.',
    ),
  ).toBeTruthy();
  expect(within(panel).queryByRole('link', { name: 'Open Run' })).toBeNull();
  expect(diagnostics.mock.calls[0]![1]).toEqual({ kind: 'group', id: 'group', version: 2 });
});

it('follows only visible authoritative card links and keeps earlier Work on bounded history pagination', async () => {
  const { data, diagnostics } = fixture();
  data.next_cursor = 'next-visible';
  const later = structuredClone(data);
  later.next_cursor = null;
  later.admissions[0]!.association.id = '55'.repeat(32);
  later.admissions[0]!.association.work_id = 'later-work';
  later.admissions[0]!.association.run_id = 'later-run';
  later.admissions[0]!.issue = {
    project_id: data.configuration.definition.project_id,
    issue_id: 'visible/task',
  };
  diagnostics.mockResolvedValueOnce(data).mockResolvedValueOnce(later);
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Load more Run history' }));
  const card = await screen.findByRole('link', { name: 'Open task card' });
  expect(card.getAttribute('href')).toBe(
    `/projects/${encodeURIComponent(later.admissions[0]!.issue!.project_id)}/issues/visible%2Ftask`,
  );
  expect(screen.getAllByRole('link', { name: 'Open Run' })).toHaveLength(2);
  expect(diagnostics.mock.calls[1]![2]).toBe('next-visible');
});

it('renders denied diagnostics as a safe failure without provider text or a false empty result', async () => {
  const { diagnostics } = fixture();
  diagnostics.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'private-provider-body-token',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  const alert = await screen.findByRole('alert');
  expect(alert.getAttribute('data-state')).toBe('forbidden');
  expect(alert.textContent).toContain(
    'Check your permissions for this operation and the selected resource',
  );
  expect(document.body.textContent).not.toContain('private-provider-body-token');
  expect(
    screen.queryByText(
      'No visible admitted Work in this page; this does not prove that no earlier or hidden execution exists.',
    ),
  ).toBeNull();
});

it('honors a bounded diagnostic retry delay while keeping provider error text private', async () => {
  const { diagnostics } = fixture();
  diagnostics.mockRejectedValue(
    new ManagementApiError({
      code: 'rate_limited',
      status: 429,
      message: 'private-rate-limit-body',
      requestId: null,
      retryAfterMilliseconds: 1000,
    }),
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  const alert = await screen.findByRole('alert');
  expect(within(alert).getByRole('button')).toHaveProperty('disabled', true);
  expect(alert.textContent).not.toContain('private-rate-limit-body');
});

it('correlates pending reporting to the exact nested stage outcome and action before Run completion', async () => {
  const data = integrationFixture('WorkSourceDiagnostics');
  const definition = configurationFixture().configuration.flow.closure.root;
  const action = {
    definition,
    node: 'notify-next',
    outcome: 'queued',
    flow_run_id: 'child-invocation',
    cycle_id: 'cycle',
    node_attempt_id: 'action-attempt',
  };
  Object.assign(data.admissions[0]!.reporting[0]!, {
    origin: {
      action,
      stages: [
        { ...action, node: 'inspect-ticket', outcome: 'usable', node_attempt_id: 'stage-attempt' },
      ],
    },
  });
  const { diagnostics } = fixture(data);
  await userEvent.click(
    await screen.findByRole('button', { name: 'Source and reporting progress' }),
  );
  const panel = await screen.findByRole('region', { name: 'Retained source progress' });
  expect(within(panel).getByText('inspect-ticket · usable')).toBeTruthy();
  const link = within(panel).getByRole('link', { name: 'notify-next · queued' });
  expect(link.getAttribute('href')).toContain('diagnostic_id=action-attempt');
  expect(panel.textContent).toContain('child-invocation');
  expect(within(panel).getByText('Awaiting external confirmation')).toBeTruthy();
  expect(diagnostics).toHaveBeenCalledTimes(1);
});
