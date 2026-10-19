// @vitest-environment jsdom
import { cleanup, screen, within, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { queryKeys } from '../../app/query';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';

afterEach(cleanup);
const card = `/projects/delivery/issues/${issue.id}`;
it('keeps manual Issues available and explains missing acceptance before any execution', async () => {
  const { api } = setup(card);
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  expect(
    await screen.findByText('Add acceptance criteria before starting a pipeline.'),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Add acceptance criteria' }));
  expect(await screen.findByRole('textbox', { name: 'Acceptance criteria' })).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
});

it('reads an exact historical execution after reload and preserves selection across card sections', async () => {
  const { executions, router } = setupExecution(`${card}?section=runs&execution=old-request`);
  executions.get.mockResolvedValue(executionFixture('admitted', 'old-request'));
  expect(await screen.findByRole('link', { name: 'Open Run' })).toBeTruthy();
  expect(executions.get).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'old-request',
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
  await userEvent.click(screen.getByRole('link', { name: 'Runs' }));
  expect(await screen.findByRole('link', { name: 'Open Run' })).toBeTruthy();
  expect(executions.start).not.toHaveBeenCalled();
});

it('resumes a lost response and pending preparation with frozen inputs even after requirements are removed', async () => {
  const { api, executions, router } = setupExecution();
  executions.start
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Response lost',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValueOnce({ disposition: 'replayed', resource: executionFixture('pending') });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(await screen.findByRole('button', { name: 'Review pipeline launch' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Retry same command' }));
  await screen.findByRole('button', { name: 'Resume preparation' });
  expect(screen.queryByRole('dialog')).toBeNull();
  await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, title: 'New requirements', acceptance: null },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await screen.findByRole('heading', { name: 'ISS-42 · New requirements' });
  expect(screen.getByRole('button', { name: 'Start pipeline' })).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Resume preparation' }));
  expect(await screen.findByRole('link', { name: 'Open Run' })).toBeTruthy();
  expect(new URLSearchParams(router.state.location.search).get('section')).toBe('activity');
  expect(executions.start).toHaveBeenCalledTimes(3);
  for (const call of executions.start.mock.calls.slice(1))
    expect(call).toEqual(executions.start.mock.calls[0]);
  expect(executions.start.mock.calls[2]?.[3]['If-Match']).toBe('"1"');
  expect(new URLSearchParams(window.location.search).get('Idempotency-Key')).toBeNull();
});

it('requires a fresh review after a stale conflict, preserves inputs, and gives later executions new keys', async () => {
  const { executions, api, router } = setupExecution();
  executions.start.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Task changed',
      status: 409,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  const first = executionFixture();
  first.intent.expected_version = 2;
  const later = executionFixture('admitted', 'request-two');
  later.intent.expected_version = 2;
  if (later.association) later.association.run_id = 'later-run';
  executions.start
    .mockResolvedValueOnce({ disposition: 'applied', resource: first })
    .mockResolvedValueOnce({ disposition: 'applied', resource: later });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source' }));
  await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Exact revision' }), 'preserved-sha');
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await screen.findByText('Task changed');
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, acceptance: 'New acceptance' },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('textbox', { name: 'Exact revision' })).toHaveProperty(
    'value',
    'preserved-sha',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  expect(screen.getByRole('dialog').textContent).toContain('Task version 2');
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await within(screen.getByRole('dialog')).findByRole('link', { name: 'Open Run' });
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await within(screen.getByRole('dialog')).findByRole('link', { name: 'Open Run' });
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('request-two');
  expect(
    within(screen.getByRole('dialog')).getByRole('link', { name: 'Open Run' }).getAttribute('href'),
  ).toBe('/factory/later-run');
  const keys = executions.start.mock.calls.map((call) => call[3]['Idempotency-Key']);
  expect(new Set(keys).size).toBe(3);
  expect(executions.start.mock.calls.map((call) => call[3]['If-Match'])).toEqual([
    '"1"',
    '"2"',
    '"2"',
  ]);
});

it('shows setup remediation and keeps execution unavailable when no pipeline is configured', async () => {
  const { listConfigurations, executions } = setupExecution();
  listConfigurations.mockResolvedValue({ items: [], next_cursor: null, total: 0 });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByText(
    'Optional Factory mode is not configured. Ordinary CI/CD remains available.',
  );
  expect(screen.getByRole('link', { name: 'Configure a pipeline' }).getAttribute('href')).toBe(
    '/factory?project_id=delivery',
  );
  expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Edit requirements' })).toHaveProperty(
    'disabled',
    false,
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it('keeps pending and earlier requirements truthful while paging and refreshing history without writes', async () => {
  const { executions } = setupExecution(`${card}?section=runs&execution=pending-request`, 2);
  executions.get.mockResolvedValue(executionFixture('pending', 'pending-request'));
  executions.list.mockResolvedValueOnce({
    items: [executionFixture()],
    next_cursor: 'opaque.cursor',
    observation: 'request-watermark-v1',
    watermark: 1,
    issue_version: 1,
  });
  expect(
    await screen.findByText(
      'This execution uses earlier requirements. Current edits apply to a later execution.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('link', { name: 'Open Run' })).toBeNull();
  await userEvent.click(await screen.findByRole('button', { name: 'Next page' }));
  await waitFor(() =>
    expect(executions.list).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      'opaque.cursor',
      expect.any(AbortSignal),
    ),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh executions' }));
  expect(executions.start).not.toHaveBeenCalled();
});

it('preserves execution selection when opening requirements history and its revision', async () => {
  const { router } = setupExecution(`${card}?execution=old-request`);
  await userEvent.click(await screen.findByRole('link', { name: 'Open full status history' }));
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
  await userEvent.click(await screen.findByRole('link', { name: 'Read task version 1' }));
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
});

it('explains disabled pipeline setup instead of offering an empty selector', async () => {
  const { listConfigurations, configuration } = setupExecution();
  listConfigurations.mockResolvedValue({
    items: [{ ...configuration, enabled: false, created_at_unix_ms: 1, published_at_unix_ms: 1 }],
    next_cursor: null,
    total: 1,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  expect(
    await screen.findByText(
      'Optional Factory mode is not configured. Ordinary CI/CD remains available.',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
});

it('retains visible target choices but disables confirmation after an unavailable refresh', async () => {
  const { projects, queryClient } = setupExecution();
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  projects.getRepository.mockRejectedValue(
    new ManagementApiError({
      code: 'unavailable',
      status: 503,
      message: 'Safe repository outage',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await queryClient.invalidateQueries({ queryKey: queryKeys.repository('repo', 2) });
  expect(await screen.findByText(/Refresh failed. Showing the last loaded/)).toBeTruthy();
  expect(screen.getByRole('combobox', { name: 'Repository' }).textContent).toContain(
    'Application source',
  );
  expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
});

it('keeps execution review independent of Issue transition-choice authority', async () => {
  const { api } = setupExecution();
  api.transitions.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Manual moves forbidden',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  expect(await screen.findByText('Manual moves forbidden')).toBeTruthy();
});

it('retains selected execution metadata with a stale notice after a failed status refresh', async () => {
  const { executions } = setupExecution(`${card}?section=runs&execution=old-request`);
  await screen.findByRole('link', { name: 'Open Run' });
  executions.get.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Status permission changed',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Refresh executions' }));
  expect(await screen.findByText(/Refresh failed. Showing the last loaded/)).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Open Run' })).toBeTruthy();
  expect(executions.start).not.toHaveBeenCalled();
});

it('does not rewrite another page when a submitted execution returns after leaving the card', async () => {
  const { router, executions } = setupExecution();
  let respond: ((value: Awaited<ReturnType<IssueExecutionsApi['start']>>) => void) | undefined;
  executions.start.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await act(async () => {
    await router.navigate('/projects/delivery/issues');
  });
  await screen.findByRole('heading', { name: 'Tasks', level: 1 });
  await act(async () => {
    respond?.({ disposition: 'applied', resource: executionFixture() });
  });
  expect(router.state.location.pathname).toBe('/projects/delivery/issues');
  expect(router.state.location.search).toBe('');
});

// Application composition reuses existing visible Factory/Repository reads.
it('reviews pinned pipeline inputs and admits through the Issue HTTP command only after confirmation', async () => {
  const { executions, configuration } = setupExecution();
  const before = JSON.stringify(localStorage);
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source' }));
  await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Exact revision' }), 'reviewed-sha');
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Task version 1');
  expect(dialog.textContent).toContain('Application source · v2');
  expect(dialog.textContent).toContain('reviewed-sha');
  expect(dialog.textContent).toContain('Check checkout works');
  expect(executions.start).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' }));
  expect((await within(dialog).findByRole('link', { name: 'Open Run' })).getAttribute('href')).toBe(
    '/factory/retained-run',
  );
  expect(executions.start).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    {
      target: {
        configuration_id: configuration.id,
        configuration_version: configuration.version,
        repository_id: 'repo',
        repository_version: 2,
        source: { kind: 'exact_revision', value: 'reviewed-sha' },
        mapping: 'title_description_acceptance_v1',
      },
      classification: { priority: 0, risk: 'low', metadata: {} },
    },
    { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
  );
  expect(JSON.stringify(localStorage)).toBe(before);
});

import { ManagementApiError } from '../../api/client';
import { factoryApi } from '../factory/api';
import { fakeProjectsApi, project } from '../projects/ProjectTestSupport';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { executionFixture } from '../../../tests/fixtures/issueExecutions';
import type { IssueExecutionsApi } from './executionApi';
function setupExecution(path = card, version = 1) {
  const configuration = {
    ...configurationFixture().configuration.reference,
    project_id: 'delivery',
  };
  const executions = {
    start: vi
      .fn<IssueExecutionsApi['start']>()
      .mockResolvedValue({ disposition: 'applied', resource: executionFixture() }),
    get: vi.fn<IssueExecutionsApi['get']>().mockResolvedValue(executionFixture()),
    list: vi.fn<IssueExecutionsApi['list']>().mockResolvedValue({
      items: [],
      next_cursor: null,
      observation: 'request-watermark-v1',
      watermark: 0,
      issue_version: 1,
    }),
  };
  const projects = fakeProjectsApi();
  projects.getProject.mockResolvedValue({
    ancestors: [],
    project: project('delivery', 'Delivery'),
  });
  projects.listRepositories.mockResolvedValue({
    items: [
      {
        id: 'repo',
        project_id: 'delivery',
        name: 'Application source',
        version: 2,
        published_at_unix_ms: 1,
      },
    ],
    next_cursor: null,
  });
  projects.getRepository.mockResolvedValue({
    id: 'repo',
    project_id: 'delivery',
    name: 'Application source',
    version: 2,
    published_at_unix_ms: 1,
    definition: {
      repository_locator: 'hidden',
      vcs_integration_id: 'vcs',
      selection: {
        default_reference: 'main',
        allowed_references: ['main'],
        allow_exact_revision: true,
      },
    },
  });
  const listConfigurations = vi.fn().mockResolvedValue({
    items: [{ ...configuration, enabled: true, created_at_unix_ms: 1, published_at_unix_ms: 1 }],
    next_cursor: null,
    total: 1,
  });
  const result = setup(
    path,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        version,
        content: { ...issue.content, acceptance: 'Check checkout works' },
      }),
    {
      issueExecutionsApi: executions,
      projectsApi: projects,
      factoryApi: { ...factoryApi, listConfigurations },
    },
  );
  return { ...result, executions, configuration, projects, listConfigurations };
}
