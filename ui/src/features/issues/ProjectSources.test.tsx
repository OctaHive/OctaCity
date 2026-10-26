// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { setup, fakeIssueSourcesApi, issue } from '../../../tests/fixtures/issueTestSupport';
import { issueSourceReview } from '../../../tests/fixtures/issueSources';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import type { IssueSourceImportIntent, IssueSourceImport } from './sourceApi';
import { ManagementApiError } from '../../api/client';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../app/presentation/preferences';
import { translate, type MessageKey } from '../../app/presentation/messages';

afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});

function fixture() {
  const sources = fakeIssueSourcesApi();
  const connection = integrationFixture('IntegrationConnection');
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  read.selection = null;
  sources.connections.mockResolvedValue({ items: [connection], next_cursor: null });
  sources.importChoices.mockResolvedValue(choices);
  sources.previewImport.mockResolvedValue({
    source: read.binding,
    items: [
      {
        display_id: 'NATIVE-7',
        item: { resource: read.native_id, item: 'opaque-7' },
        title: 'Investigate native checkout',
        state: 'native-open',
        labels: [],
      },
    ],
    has_more: false,
    failure: null,
    observed_at_unix_ms: 1,
    retry_after_millis: null,
  });
  const result = setup('/projects/delivery/issues', undefined, { issueSourcesApi: sources });
  return { ...result, sources, connection, choices, read };
}

async function choose(label: string, option: string) {
  await userEvent.click(await screen.findByRole('combobox', { name: label }));
  await userEvent.click(await screen.findByRole('option', { name: option }));
}

it('previews a Project task source before any Issue or Factory target exists without activating import or execution', async () => {
  const { api, sources, connection, read, choices, queryClient } = fixture();
  await screen.findByText('No tasks yet. Create your first task here.');
  expect(sources.connections).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Task sources' }));
  expect(screen.getByRole('link', { name: 'Workflow' }).getAttribute('href')).toBe(
    '/projects/delivery/issue-workflows',
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Add task source' }));
  await choose('Source connection', connection.definition.name);
  await choose(
    'Source profile',
    `${read.locator} · ${read.binding.configuration.identity} / ${read.binding.configuration.version}`,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Preview tasks' }));
  const preview = await screen.findByRole('region', { name: 'Task source preview' });
  expect(within(preview).getByText('NATIVE-7 · Investigate native checkout')).toBeTruthy();
  expect(sources.previewImport).toHaveBeenCalledWith('delivery', {
    connection_id: connection.id,
    connection_version: connection.version,
    access_version: choices.access_version,
    source: read.binding,
    limit: 10,
  });
  expect(sources.publishImport).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(
    JSON.stringify(
      queryClient
        .getQueryCache()
        .getAll()
        .map((query) => query.state.data),
    ),
  ).not.toContain('Investigate native checkout');
  expect(screen.queryByRole('textbox', { name: /token|repository|target/i })).toBeNull();
  expect(screen.getByRole('button', { name: 'Create task' })).toHaveProperty('disabled', false);
});

it('distinguishes coexisting installed profile versions and previews only the explicitly selected pin', async () => {
  const { sources, read, choices, connection } = fixture();
  const next = {
    ...read,
    binding: {
      ...read.binding,
      configuration: { ...read.binding.configuration, version: 'v2', digest: '22'.repeat(32) },
    },
  };
  sources.importChoices.mockResolvedValue({ ...choices, items: [read, next] });
  await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add task source' }));
  await choose('Source connection', connection.definition.name);
  await choose('Source profile', `${read.locator} · ${read.binding.configuration.identity} / v2`);
  await userEvent.click(screen.getByRole('button', { name: 'Preview tasks' }));
  await waitFor(() =>
    expect(sources.previewImport).toHaveBeenCalledWith(
      'delivery',
      expect.objectContaining({ source: next.binding }),
    ),
  );
  expect(sources.publishImport).not.toHaveBeenCalled();
});

it('explains absent connections without blocking manual task creation or inventing an import', async () => {
  const { sources, api } = fixture();
  sources.connections.mockResolvedValue({ items: [], next_cursor: null });
  await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
  await screen.findByText(
    'No usable source is configured. Set up a connection and Project access in Integrations.',
  );
  expect(screen.getByRole('button', { name: 'Create task' })).toHaveProperty('disabled', false);
  expect(screen.getByRole('button', { name: 'Add task source' })).toHaveProperty('disabled', true);
  expect(sources.importChoices).not.toHaveBeenCalled();
  expect(sources.publishImport).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
});

it('publishes explicitly enabled import only after frozen review and shows its retained configuration without claiming imported tasks', async () => {
  const { sources, connection, read, choices, api } = fixture();
  sources.publishImport.mockImplementation(async (intent: IssueSourceImportIntent) => {
    const configuration = {
      id: intent.id,
      project_id: intent.project_id,
      version: 1,
      published_at: 1,
      definition: intent.definition,
    };
    sources.imports.mockResolvedValue({ items: [configuration], next_cursor: null });
    return { disposition: 'applied', configuration };
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add task source' }));
  await choose('Source connection', connection.definition.name);
  await choose(
    'Source profile',
    `${read.locator} · ${read.binding.configuration.identity} / ${read.binding.configuration.version}`,
  );
  const enabled = screen.getByRole('checkbox', { name: 'Discover tasks automatically' });
  expect(enabled).toHaveProperty('checked', false);
  await userEvent.click(enabled);
  await userEvent.click(screen.getByRole('button', { name: 'Review task import' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish task import' });
  expect(dialog.textContent).toContain(connection.definition.name);
  expect(dialog.textContent).toContain(read.locator);
  expect(sources.publishImport).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm task import' }));
  await within(dialog).findByText(
    'Import settings published. Refresh tasks to see imported tasks.',
  );
  expect(sources.publishImport).toHaveBeenCalledWith(
    {
      project_id: 'delivery',
      id: expect.any(String),
      expected_current_version: null,
      definition: {
        enabled: true,
        source: read.binding,
        connection: {
          id: connection.id,
          version: connection.version,
          access_version: choices.access_version,
          source_configuration: read.connection_configuration,
          reporter_configuration: null,
        },
      },
    },
    { 'Idempotency-Key': expect.any(String) },
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(await screen.findByText('Task discovery enabled')).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
  expect(screen.getByText('No tasks yet. Create your first task here.')).toBeTruthy();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Add task source' }));
});

it('keeps an import draft through stale access and requires explicit profile selection and a fresh review after refresh', async () => {
  const { sources, connection, choices, read } = fixture();
  const replacement = {
    ...read,
    binding: {
      ...read.binding,
      configuration: { ...read.binding.configuration, version: 'v2', digest: '22'.repeat(32) },
    },
    connection_configuration: {
      ...read.connection_configuration,
      version: 'v2',
      digest: '33'.repeat(32),
    },
  };
  sources.publishImport
    .mockImplementationOnce(async () => {
      sources.connections.mockResolvedValue({
        items: [{ ...connection, version: connection.version + 1 }],
        next_cursor: null,
      });
      sources.importChoices.mockResolvedValue({
        ...choices,
        access_version: choices.access_version + 1,
        items: [replacement],
      });
      throw new ManagementApiError({
        code: 'precondition_failed',
        status: 412,
        requestId: null,
        retryAfterMilliseconds: null,
        message: 'private-access-detail',
      });
    })
    .mockImplementation(async (intent: IssueSourceImportIntent) => ({
      disposition: 'applied',
      configuration: {
        id: intent.id,
        project_id: intent.project_id,
        definition: intent.definition,
        version: 1,
        published_at: 1,
      },
    }));
  await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add task source' }));
  await choose('Source connection', connection.definition.name);
  await choose(
    'Source profile',
    `${read.locator} · ${read.binding.configuration.identity} / ${read.binding.configuration.version}`,
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Discover tasks automatically' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review task import' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Confirm task import' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('checkbox', { name: 'Discover tasks automatically' })).toHaveProperty(
    'checked',
    true,
  );
  expect(screen.getByRole('button', { name: 'Review task import' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(sources.publishImport).toHaveBeenCalledOnce();
  await choose('Source profile', `${read.locator} · ${read.binding.configuration.identity} / v2`);
  await userEvent.click(screen.getByRole('button', { name: 'Review task import' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Confirm task import' }),
  );
  await within(screen.getByRole('dialog')).findByText(
    'Import settings published. Refresh tasks to see imported tasks.',
  );
  const [first, second] = sources.publishImport.mock.calls;
  expect(second![0]).toMatchObject({
    id: first![0].id,
    expected_current_version: null,
    definition: {
      enabled: true,
      source: replacement.binding,
      connection: {
        id: connection.id,
        version: connection.version + 1,
        access_version: choices.access_version + 1,
        source_configuration: replacement.connection_configuration,
      },
    },
  });
  expect(second![1]['Idempotency-Key']).not.toBe(first![1]['Idempotency-Key']);
  expect(document.body.textContent).not.toContain('private-access-detail');
});

it.each([true, false])(
  'reviews lifecycle changes from enabled=%s with exact pins and safe replay, without changing linked task synchronization',
  async (enabled) => {
    const { sources, read, connection, choices, api } = fixture();
    const original: IssueSourceImport = {
      project_id: 'delivery',
      id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
      version: 2,
      published_at: 1,
      definition: {
        enabled,
        source: read.binding,
        predicate: {
          state: 'native-open',
          all_of: ['keep-filter'],
          any_of: ['one-of-these'],
          none_of: ['leave-out'],
        },
        connection: {
          id: connection.id,
          version: connection.version,
          access_version: choices.access_version,
          source_configuration: read.connection_configuration,
          reporter_configuration: null,
        },
      },
    };
    sources.imports.mockResolvedValue({ items: [original], next_cursor: null });
    sources.publishImport
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'transport_failure',
          status: null,
          requestId: null,
          retryAfterMilliseconds: null,
          message: 'private-import-token',
        }),
      )
      .mockImplementation(async (intent: IssueSourceImportIntent) => {
        const configuration = { ...original, version: 3, definition: intent.definition };
        sources.imports.mockResolvedValue({ items: [configuration], next_cursor: null });
        return { configuration, disposition: 'applied' };
      });
    await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
    const action = await screen.findByRole('button', {
      name: enabled ? 'Disable task import' : 'Enable task import',
    });
    await userEvent.click(action);
    const dialog = await screen.findByRole('dialog', { name: 'Publish task import' });
    if (enabled) expect(sources.importChoices).not.toHaveBeenCalled();
    expect(dialog.textContent).toContain('keep-filter');
    expect(dialog.textContent).toContain('All required labels');
    expect(dialog.textContent).toContain('Any matching labels');
    expect(dialog.textContent).toContain('Excluded labels');
    expect(sources.publishImport).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm task import' }));
    await userEvent.click(
      await within(dialog).findByRole('button', { name: 'Retry same command' }),
    );
    await within(dialog).findByText(
      'Import settings published. Refresh tasks to see imported tasks.',
    );
    expect(sources.publishImport.mock.calls).toHaveLength(2);
    expect(sources.publishImport.mock.calls[0]).toEqual([
      {
        id: original.id,
        project_id: 'delivery',
        expected_current_version: 2,
        definition: { ...original.definition, enabled: !enabled },
      },
      { 'Idempotency-Key': expect.any(String) },
    ]);
    expect(sources.publishImport.mock.calls[1]).toEqual(sources.publishImport.mock.calls[0]);
    expect(document.body.textContent).not.toContain('private-import-token');
    expect(sources.setPull).not.toHaveBeenCalled();
    expect(sources.publishMapping).not.toHaveBeenCalled();
    expect(sources.previewImport).not.toHaveBeenCalled();
    expect(api.create).not.toHaveBeenCalled();
  },
);

it('refreshes the canonical task list after discovery and opens the existing source view without manufacturing a task', async () => {
  const { api, sources } = fixture();
  await screen.findByText('No tasks yet. Create your first task here.');
  api.list.mockResolvedValue({
    items: [{ id: issue.id, display_key: issue.display_key, title: 'Investigate native checkout' }],
    next_cursor: null,
  });
  api.get.mockResolvedValue({
    ...issue,
    content: { ...issue.content, title: 'Investigate native checkout' },
  });
  sources.review.mockResolvedValue(issueSourceReview(true));
  await userEvent.click(screen.getByRole('button', { name: 'Refresh tasks' }));
  const link = await screen.findByRole('link', { name: 'ISS-42 · Investigate native checkout' });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh tasks' }));
  expect(
    screen.getAllByRole('link', { name: 'ISS-42 · Investigate native checkout' }),
  ).toHaveLength(1);
  await userEvent.click(link);
  await screen.findByRole('heading', { name: 'ISS-42 · Investigate native checkout' });
  await screen.findByRole('region', { name: 'Source and synchronization' });
  expect(await screen.findByText('opaque-7')).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
  expect(sources.refresh).not.toHaveBeenCalled();
  expect(sources.publishMapping).not.toHaveBeenCalled();
});

it('refreshes current access for re-enabling the same retained import after a grant revision', async () => {
  const { sources, read, connection, choices } = fixture();
  const original: IssueSourceImport = {
    project_id: 'delivery',
    id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
    version: 2,
    published_at: 1,
    definition: {
      enabled: false,
      source: read.binding,
      predicate: { state: null, all_of: ['keep'], any_of: [], none_of: [] },
      connection: {
        id: connection.id,
        version: connection.version,
        access_version: choices.access_version,
        source_configuration: read.connection_configuration,
        reporter_configuration: null,
      },
    },
  };
  sources.imports.mockResolvedValue({ items: [original], next_cursor: null });
  sources.importChoices
    .mockResolvedValueOnce(choices)
    .mockResolvedValue({ ...choices, access_version: choices.access_version + 1 });
  sources.publishImport.mockImplementation(async (intent) => {
    if (intent.definition.connection.access_version !== choices.access_version + 1)
      throw new ManagementApiError({
        code: 'precondition_failed',
        status: 412,
        requestId: null,
        retryAfterMilliseconds: null,
        message: 'Access revision changed',
      });
    return {
      disposition: 'applied',
      configuration: { ...original, version: 3, definition: intent.definition },
    };
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Enable task import' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Confirm task import' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  await userEvent.click(screen.getByRole('button', { name: 'Enable task import' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Confirm task import' }),
  );
  await waitFor(() => expect(sources.publishImport).toHaveBeenCalledTimes(2));
  await within(screen.getByRole('dialog')).findByText(
    'Import settings published. Refresh tasks to see imported tasks.',
  );
  expect(sources.publishImport.mock.calls[1]![0]).toMatchObject({
    id: original.id,
    expected_current_version: original.version,
    definition: {
      predicate: original.definition.predicate,
      connection: { access_version: choices.access_version + 1 },
    },
  });
  expect(sources.publishImport.mock.calls[1]![0].definition.source).toEqual(
    original.definition.source,
  );
  expect(sources.publishImport.mock.calls[1]![1]['Idempotency-Key']).not.toBe(
    sources.publishImport.mock.calls[0]![1]['Idempotency-Key'],
  );
  expect(sources.previewImport).not.toHaveBeenCalled();
});

it.each(['Source', 'connection configuration'] as const)(
  'blocks re-enable when the exact retained %s pin is no longer authorized and retries without adopting its replacement',
  async (parent) => {
    const { sources, read, connection, choices } = fixture();
    const original: IssueSourceImport = {
      project_id: 'delivery',
      id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
      version: 2,
      published_at: 1,
      definition: {
        enabled: false,
        source: read.binding,
        connection: {
          id: connection.id,
          version: connection.version,
          access_version: choices.access_version,
          source_configuration: read.connection_configuration,
          reporter_configuration: null,
        },
      },
    };
    const replacement = structuredClone(read);
    if (parent === 'Source') replacement.binding.configuration.version = 'v2';
    else replacement.connection_configuration.version = 'v2';
    sources.imports.mockResolvedValue({ items: [original], next_cursor: null });
    sources.importChoices.mockResolvedValue({ ...choices, items: [replacement] });
    await userEvent.click(await screen.findByRole('button', { name: 'Task sources' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Enable task import' }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('installed Source runtime/profile');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(sources.publishImport).not.toHaveBeenCalled();
    sources.importChoices.mockResolvedValue(choices);
    await userEvent.click(within(alert).getByRole('button', { name: 'Retry' }));
    await screen.findByRole('dialog', { name: 'Publish task import' });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(sources.publishImport).not.toHaveBeenCalled();
    expect(original.definition.source.configuration.version).toBe('v1');
  },
);

for (const language of ['en', 'ru'] as const) {
  it.each(['invalid_request', 'precondition_failed'] as const)(
    `explains Project source %s through available controls in ${language}`,
    async (code) => {
      localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ...DEFAULT_PREFERENCES, language }));
      const { sources, connection, read } = fixture();
      const text = (key: MessageKey) => translate(language, key);
      sources.previewImport.mockRejectedValueOnce(
        new ManagementApiError({
          code,
          status: code === 'invalid_request' ? 422 : 412,
          requestId: null,
          retryAfterMilliseconds: null,
          message: 'private-provider-text',
        }),
      );
      await userEvent.click(await screen.findByRole('button', { name: text('issueImport.open') }));
      await userEvent.click(await screen.findByRole('button', { name: text('issueImport.add') }));
      await choose(text('factory.sourceConnection'), connection.definition.name);
      await choose(
        text('issueSource.profile'),
        `${read.locator} · ${read.binding.configuration.identity} / v1`,
      );
      await userEvent.click(screen.getByRole('button', { name: text('issueImport.preview') }));
      const alert = await screen.findByRole('alert');
      const expected =
        code === 'invalid_request'
          ? language === 'en'
            ? 'Check the selected configuration and supported values'
            : 'Проверьте выбранные настройки и допустимые значения'
          : language === 'en'
            ? 'Refresh current settings and review the operation again'
            : 'Обновите текущие настройки и заново просмотрите операцию';
      expect(alert.textContent).toContain(expected);
      expect(alert.textContent).not.toContain(
        language === 'en' ? 'validate' : 'повторите проверку',
      );
      expect(alert.textContent).not.toContain('private-provider-text');
      await userEvent.click(within(alert).getByRole('button', { name: text('query.retry') }));
      await screen.findByRole('region', { name: text('issueImport.previewTitle') });
      expect(screen.queryByRole('alert')).toBeNull();
      expect(sources.publishImport).not.toHaveBeenCalled();
    },
  );
}
