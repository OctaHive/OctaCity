// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { factorySourcesApi, type FactorySourcesApi, type SourceIntent } from './sourceApi';
import { ManagementApiError } from '../../api/client';
import { FactoryPipelineInputs } from './FactoryPipelineInputs';

afterEach(cleanup);
function fixture(overrides: Partial<FactorySourcesApi> = {}) {
  const configuration = configurationFixture();
  const connection = integrationFixture('IntegrationConnection');
  const choice = integrationFixture('WorkSourceInputChoices');
  const read = choice.items.find((i) => i.purpose === 'task_read')!;
  const code = {
    id: '33333333-3333-4333-8333-333333333333',
    version: 3,
    project_id: configuration.configuration.reference.project_id,
    name: 'Code in another host',
    published_at_unix_ms: 1,
  };
  const repositories = {
    listRepositories: vi.fn(async () => ({ items: [code], next_cursor: null })),
    getRepository: vi.fn(async () => ({
      ...code,
      definition_digest: 'ab'.repeat(32),
      definition: {
        vcs_integration_id: '55555555-5555-4555-8555-555555555555',
        repository_locator: 'https://code.invalid/city.git',
        selection: {
          allowed_references: ['main'],
          default_reference: 'main',
          allow_exact_revision: false,
        },
      },
    })),
  };
  const publication = (intent: SourceIntent) => ({
    project_id: intent.project_id,
    inputs: intent.inputs.map((i) => ({
      expected_version: i.expected_version ?? null,
      input: {
        id: i.id,
        version: (i.expected_version ?? 0) + 1,
        project_id: intent.project_id,
        root_flow: configuration.configuration.flow.closure.root,
        definition: i.definition,
      },
    })),
    group: null,
  });
  const validate = vi.fn<FactorySourcesApi['validate']>(async (intent) => ({
    publication: publication(intent),
  }));
  const publish = vi.fn<FactorySourcesApi['publish']>(async (intent) => ({
    disposition: 'applied',
    publication: publication(intent),
    published_at_unix_ms: 1,
  }));
  const api = {
    ...factorySourcesApi,
    list: vi.fn(async () => ({ items: [], next_cursor: null })),
    connections: vi.fn(async () => ({ items: [connection], next_cursor: null })),
    choices: vi.fn(async () => ({
      ...choice,
      items: [{ ...read, predicate_states: ['native-open', 'native-closed'] }],
    })),
    validate,
    publish,
    preview: vi.fn<FactorySourcesApi['preview']>(async (intent: SourceIntent) => ({
      ...integrationFixture('WorkSourcePreview'),
      configuration: intent,
    })),
    ...overrides,
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: (
              <FactoryPipelineInputs
                configuration={configuration}
                api={api}
                repositories={repositories}
                onManual={() => {}}
              />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  return { api, validate, publish, connection, choice, read, code, repositories };
}
async function choose(label: string, option: string) {
  await userEvent.click(await screen.findByRole('combobox', { name: label }));
  await userEvent.click(await screen.findByRole('option', { name: option }));
}
async function openInput() {
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add automatic input' }));
}
it('selects an authorized task container independently of code, previews read-only and publishes only a freshly validated reviewed input', async () => {
  const { api, validate, publish, connection, read, code } = fixture();
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', read.binding.configuration.identity);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize filters' }));
  await choose('Task state', 'native-open');
  await userEvent.type(
    screen.getByLabelText('All required labels (one per line)'),
    'needs-human-review\ncommas, are literal',
  );
  expect(screen.queryByLabelText('Credential value')).toBeNull();
  expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
  expect(
    await screen.findByText('Preview does not import tasks, reserve execution or start a Run.'),
  ).toBeTruthy();
  expect(api.preview).toHaveBeenCalledOnce();
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  expect(dialog.textContent).toContain('commas, are literal');
  expect(dialog.textContent).toContain(`Source connection: ${connection.definition.name}`);
  expect(dialog.textContent).not.toContain('source_configuration');
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  expect(await within(dialog).findByText('Automatic input published.')).toBeTruthy();
  const submitted = validate.mock.calls[0]![0].inputs[0]!.definition;
  expect(submitted.connection?.id).toBe(connection.id);
  expect(submitted.source.resource).toEqual(read.binding.resource);
  expect(submitted.repository).toEqual({
    id: code.id,
    version: 3,
    project_id: code.project_id,
    definition_digest: 'ab'.repeat(32),
  });
  expect(submitted.predicate).toEqual({
    state: 'native-open',
    all_of: ['needs-human-review', 'commas, are literal'],
    any_of: [],
    none_of: [],
  });
  expect(submitted.enabled).toBe(false);
});

it('groups retained input versions explicitly and previews ambiguity without replacing inputs', async () => {
  const intent = integrationFixture('WorkSourceConfigurationSubmission').intent;
  const configuration = configurationFixture();
  const first = {
    ...intent.inputs[0]!,
    definition: { ...intent.inputs[0]!.definition, target: configuration.configuration.reference },
  };
  const records: Awaited<ReturnType<FactorySourcesApi['list']>>['items'] = [1, 2].map((i) => ({
    kind: 'input',
    definition: {
      id: `00000000-0000-4000-8000-00000000000${i}`,
      version: i,
      project_id: configuration.configuration.reference.project_id,
      root_flow: configuration.configuration.flow.closure.root,
      definition: first.definition,
    },
  }));
  const preview = integrationFixture('WorkSourcePreview');
  preview.candidates[0]!.disposition = 'ambiguous';
  const groupPublish = vi.fn<FactorySourcesApi['publish']>(async (draft) => ({
    disposition: 'applied',
    published_at_unix_ms: 1,
    publication: {
      project_id: draft.project_id,
      inputs: [],
      group: {
        expected_version: null,
        group: {
          id: draft.group!.id,
          version: 1,
          project_id: draft.project_id,
          members: records.flatMap((record) =>
            record.kind === 'input' ? [record.definition] : [],
          ),
        },
      },
    },
  }));
  const { api, validate } = fixture({
    publish: groupPublish,
    list: vi.fn(async () => ({ items: records, next_cursor: null })),
    preview: vi.fn<FactorySourcesApi['preview']>(async (draft) => ({
      ...preview,
      configuration: draft,
    })),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add exclusive input group' }));
  const members = await screen.findAllByRole('checkbox');
  await userEvent.click(members[0]!);
  await userEvent.click(members[1]!);
  await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
  expect(
    await screen.findByText('Ambiguous: more than one exclusive input matches', { exact: false }),
  ).toBeTruthy();
  expect(groupPublish).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  expect(await screen.findByRole('dialog', { name: 'Publish exclusive input group' })).toBeTruthy();
  expect(validate.mock.calls[0]![0].inputs).toEqual([]);
  expect(validate.mock.calls[0]![0].group?.members).toEqual(
    records.map((record) => ({ id: record.definition.id, version: record.definition.version })),
  );
  expect(api.preview).toHaveBeenCalledWith(validate.mock.calls[0]![0]);
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Confirm publication' }),
  );
  expect(
    await within(screen.getByRole('dialog')).findByText('Automatic input published.'),
  ).toBeTruthy();
  expect(groupPublish).toHaveBeenCalledOnce();
  expect(groupPublish.mock.calls[0]![0]).toEqual(validate.mock.calls[0]![0]);
});

function retainedInput() {
  const configuration = configurationFixture();
  const source = integrationFixture('WorkSourceConfigurationSubmission').intent.inputs[0]!;
  return {
    kind: 'input' as const,
    definition: {
      id: source.id,
      version: 1,
      project_id: configuration.configuration.reference.project_id,
      root_flow: configuration.configuration.flow.closure.root,
      definition: {
        ...source.definition,
        target: configuration.configuration.reference,
        repository: {
          id: '33333333-3333-4333-8333-333333333333',
          version: 3,
          project_id: configuration.configuration.reference.project_id,
          definition_digest: 'ab'.repeat(32),
        },
        predicate: { state: null, all_of: ['retained-label'], any_of: [], none_of: [] },
      },
    },
  };
}
it('does not silently remove a retained filter when installed editing support is withdrawn', async () => {
  const record = retainedInput();
  fixture({
    list: vi.fn(async () => ({ items: [record], next_cursor: null })),
    choices: vi.fn(async () => integrationFixture('WorkSourceInputChoices')),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  await screen.findByText(
    'Saved filters are unavailable in the installed profile. Publication is blocked until an authorized compatible profile is selected.',
  );
  expect(screen.getByText(/retained-label/)).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty('disabled', true);
});

it('refreshes changed connection and access after conflict while keeping filters and requiring a new review', async () => {
  const original = retainedInput();
  let stale = false;
  const conflict = () =>
    new ManagementApiError({
      code: 'precondition_failed',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
      message: 'unsafe remote text',
    });
  const { validate, api } = fixture({
    list: vi.fn(async () => ({
      items: [{ ...original, definition: { ...original.definition, version: stale ? 2 : 1 } }],
      next_cursor: null,
    })),
    connections: vi.fn(async () => ({
      items: [{ ...integrationFixture('IntegrationConnection'), version: stale ? 2 : 1 }],
      next_cursor: null,
    })),
    choices: vi.fn(async (_project, _connection, version) => {
      if (stale && version === 1) throw conflict();
      const choices = integrationFixture('WorkSourceInputChoices');
      return {
        ...choices,
        connection_version: version,
        access_version: stale ? 3 : 2,
        items: choices.items.map((choice) => ({ ...choice, predicate_states: ['native-open'] })),
      };
    }),
    publish: vi.fn(async () => {
      stale = true;
      throw conflict();
    }),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  const labels = await screen.findByLabelText('All required labels (one per line)');
  await userEvent.clear(labels);
  await userEvent.type(labels, 'my draft, stays');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publication' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByLabelText('All required labels (one per line)')).toHaveProperty(
    'value',
    'my draft, stays',
  );
  expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.queryByText('unsafe remote text')).toBeNull();
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  const reconciled = validate.mock.calls.at(-1)![0].inputs[0]!;
  expect(reconciled.expected_version).toBe(2);
  expect(reconciled.definition.connection).toMatchObject({ version: 2, access_version: 3 });
  expect(reconciled.definition.predicate?.all_of).toEqual(['my draft, stays']);
  expect(api.publish).toHaveBeenCalledOnce();
});

it('reuses one connection for separate disabled inputs without credentials and discards preview and draft on close', async () => {
  const { api, connection, read, publish } = fixture();
  for (const label of ['first-input-only', 'second-input-only']) {
    await openInput();
    await choose('Source connection', connection.definition.name);
    await choose('Task container', read.locator);
    await choose('Selector and input mapping', read.binding.configuration.identity);
    await choose('Code Repository', 'Code in another host · v3');
    await userEvent.click(screen.getByRole('checkbox', { name: 'Customize filters' }));
    await userEvent.type(screen.getByLabelText('Any matching labels (one per line)'), label);
    await userEvent.type(screen.getByLabelText('Excluded labels (one per line)'), 'leave-alone');
    await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
    await userEvent.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Confirm publication',
      }),
    );
    await userEvent.click(
      await within(screen.getByRole('dialog')).findByRole('button', { name: 'Close' }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Close input editor' }));
    // Hide and reopen the section: no draft or preview is restored.
    await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  }
  const [first, second] = publish.mock.calls.map(([intent]) => intent.inputs[0]!);
  expect(first!.id).not.toBe(second!.id);
  expect(first!.definition.connection).toEqual(second!.definition.connection);
  expect(second!.definition.predicate).toEqual({
    state: null,
    all_of: [],
    any_of: ['second-input-only'],
    none_of: ['leave-alone'],
  });
  expect(api.choices).toHaveBeenCalled();
  expect(screen.queryByText('first-input-only')).toBeNull();
  expect(
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  ).not.toContain('input-only');
});

it('explains unrestricted state and labels including unlabeled tasks', async () => {
  const { connection, read } = fixture();
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', read.binding.configuration.identity);
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize filters' }));
  expect(screen.getByRole('combobox', { name: 'Task state' }).textContent).toContain(
    'Any state (no extra restriction)',
  );
  expect(
    screen.getByText('No label restrictions: tasks without labels are eligible too.'),
  ).toBeTruthy();
});

it('selects the exact installed profile even when separate profile identities share content', async () => {
  const choices = integrationFixture('WorkSourceInputChoices');
  const original = choices.items[0]!;
  const second = {
    ...original,
    binding: {
      ...original.binding,
      configuration: { ...original.binding.configuration, identity: 'second-mapping-same-content' },
    },
  };
  const { connection, read, validate } = fixture({
    choices: vi.fn(async () => ({ ...choices, items: [original, second] })),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', 'second-mapping-same-content');
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(validate.mock.calls[0]![0].inputs[0]!.definition.source.configuration.identity).toBe(
    'second-mapping-same-content',
  );
});

it('explains a filtered preview using translated reasons and distinguishes an unavailable source from empty results', async () => {
  const preview = integrationFixture('WorkSourcePreview');
  preview.candidates[0]!.disposition = 'filtered';
  preview.candidates[0]!.summary.selections[0]!.reasons = ['required_label_missing'];
  let unavailable = false;
  const { connection, read } = fixture({
    preview: vi.fn<FactorySourcesApi['preview']>(async (intent) =>
      unavailable
        ? { ...preview, configuration: intent, failure: 'unavailable', candidates: [] }
        : { ...preview, configuration: intent },
    ),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', read.binding.configuration.identity);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
  expect(await screen.findByText(/A required label is missing/)).toBeTruthy();
  expect(screen.queryByText(/required_label_missing/)).toBeNull();
  unavailable = true;
  await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
  expect(
    await screen.findByText(/Source preview is unavailable; this is not an empty result/),
  ).toBeTruthy();
  expect(screen.queryByText('No tasks returned by this bounded read.')).toBeNull();
});

it('keeps every selected group member visible and removable when a conflicting refresh advances current inputs', async () => {
  const first = retainedInput().definition;
  const second = { ...first, id: '44444444-4444-4444-8444-444444444444' };
  const third = { ...first, id: '66666666-6666-4666-8666-666666666666' };
  let stale = false;
  const { api, validate } = fixture({
    list: vi.fn(async () => {
      const members = [first, second].map((input) => ({ ...input, version: stale ? 2 : 1 }));
      return {
        items: [
          ...[...members, { ...third, version: stale ? 2 : 1 }].map((input) => ({
            kind: 'input' as const,
            definition: input,
          })),
          {
            kind: 'group' as const,
            definition: {
              id: '55555555-5555-4555-8555-555555555555',
              version: stale ? 2 : 1,
              project_id: first.project_id,
              members,
            },
          },
        ],
        next_cursor: null,
      };
    }),
    publish: vi.fn(async () => {
      stale = true;
      throw new ManagementApiError({
        code: 'precondition_failed',
        status: 412,
        requestId: null,
        retryAfterMilliseconds: null,
        message: 'version changed',
      });
    }),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit exclusive input group' }));
  await userEvent.click(screen.getByRole('checkbox', { name: new RegExp(`${third.id} · v1`) }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publication' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('checkbox', { name: new RegExp(`${first.id} · v1`) })).toHaveProperty(
    'checked',
    true,
  );
  expect(screen.getByRole('checkbox', { name: new RegExp(`${second.id} · v1`) })).toHaveProperty(
    'checked',
    true,
  );
  expect(screen.getByRole('checkbox', { name: new RegExp(`${third.id} · v1`) })).toHaveProperty(
    'checked',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  expect(validate.mock.calls.at(-1)![0].group).toMatchObject({
    expected_version: 2,
    members: [
      { id: first.id, version: 1 },
      { id: second.id, version: 1 },
      { id: third.id, version: 1 },
    ],
  });
  expect(api.publish).toHaveBeenCalledOnce();
  await userEvent.click(screen.getByRole('checkbox', { name: new RegExp(`${third.id} · v1`) }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(validate.mock.calls.at(-1)![0].group?.members).toEqual([
      { id: first.id, version: 1 },
      { id: second.id, version: 1 },
    ]),
  );
});

function reportedInput() {
  const record = retainedInput();
  const source = record.definition.definition.source;
  record.definition.definition.reporter = {
    ...source,
    capability: { ...source.capability, identity: 'write-task' },
    configuration: { ...source.configuration, identity: 'status-mapping' },
  };
  record.definition.definition.connection!.reporter_configuration = {
    ...source.configuration,
    identity: 'reporter-parent',
  };
  return record;
}

it.each(['container', 'installation', 'connection'] as const)(
  'removes the old Reporter from a changed %s scope only in the reviewed new version',
  async (scope) => {
    const record = reportedInput();
    const choices = integrationFixture('WorkSourceInputChoices');
    const original = choices.items[0]!;
    const alternative = {
      ...original,
      locator: 'another-team/tasks',
      binding: {
        ...original.binding,
        resource:
          scope === 'container'
            ? { ...original.binding.resource, identity: 'other-task-container' }
            : original.binding.resource,
        installation:
          scope === 'installation'
            ? { ...original.binding.installation, version: 'v2', digest: '09'.repeat(32) }
            : original.binding.installation,
        configuration: { ...original.binding.configuration, identity: 'other-mapping' },
      },
    };
    const originalConnection = integrationFixture('IntegrationConnection');
    const otherConnection = {
      ...originalConnection,
      id: '77777777-7777-4777-8777-777777777777',
      definition: { ...originalConnection.definition, name: 'Another tracker' },
    };
    const { validate, publish } = fixture({
      list: vi.fn(async () => ({ items: [record], next_cursor: null })),
      connections: vi.fn(async () => ({
        items: [originalConnection, otherConnection],
        next_cursor: null,
      })),
      choices: vi.fn(async (_project, connectionId, version) => ({
        ...choices,
        connection_id: connectionId,
        connection_version: version,
        items: [original, alternative],
      })),
    });
    await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
    if (scope === 'connection') await choose('Source connection', 'Another tracker');
    await choose('Task container', alternative.locator);
    await choose('Selector and input mapping', 'other-mapping');
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
    const draft = validate.mock.calls[0]![0].inputs[0]!;
    expect(draft.definition.reporter).toBeNull();
    expect(draft.definition.connection?.reporter_configuration).toBeNull();
    expect(draft.id).toBe(record.definition.id);
    expect(draft.expected_version).toBe(1);
    expect(
      screen.getByText('The source scope changed. This version will not report external status.'),
    ).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('Reporting: none');
    expect(publish).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
    expect(await within(dialog).findByText('Automatic input published.')).toBeTruthy();
    expect(publish.mock.calls[0]![0].inputs[0]!.definition.reporter).toBeNull();
    expect(record.definition.definition.reporter?.configuration.identity).toBe('status-mapping');
  },
);

it('preserves the retained Reporter and its parent when only the source mapping changes', async () => {
  const record = reportedInput();
  const choices = integrationFixture('WorkSourceInputChoices');
  const original = choices.items[0]!;
  const alternative = {
    ...original,
    binding: {
      ...original.binding,
      configuration: { ...original.binding.configuration, identity: 'another-read-mapping' },
    },
  };
  const { validate } = fixture({
    list: vi.fn(async () => ({ items: [record], next_cursor: null })),
    choices: vi.fn(async () => ({ ...choices, items: [original, alternative] })),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  await choose('Selector and input mapping', 'another-read-mapping');
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  const draft = validate.mock.calls[0]![0].inputs[0]!.definition;
  expect(draft.reporter).toEqual(record.definition.definition.reporter);
  expect(draft.connection?.reporter_configuration).toEqual(
    record.definition.definition.connection?.reporter_configuration,
  );
  expect(
    screen.queryByText('The source scope changed. This version will not report external status.'),
  ).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  expect((await screen.findByRole('dialog')).textContent).toContain('Reporting: status-mapping');
});

it('refreshes a new input after a connection conflict without requiring an existing publication', async () => {
  let stale = false;
  const { api, connection, read, validate, publish } = fixture({
    connections: vi.fn(async () => ({
      items: [{ ...integrationFixture('IntegrationConnection'), version: stale ? 2 : 1 }],
      next_cursor: null,
    })),
    choices: vi.fn(async (_project, _connection, version) => {
      const choices = integrationFixture('WorkSourceInputChoices');
      return {
        ...choices,
        connection_version: version,
        access_version: stale ? 3 : 2,
        items: choices.items.map((item) => ({ ...item, predicate_states: ['native-open'] })),
      };
    }),
  });
  publish.mockImplementationOnce(async () => {
    stale = true;
    throw new ManagementApiError({
      code: 'precondition_failed',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
      message: 'metadata changed',
    });
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', read.binding.configuration.identity);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize filters' }));
  await userEvent.type(
    screen.getByLabelText('All required labels (one per line)'),
    'keep-new-draft',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  const original = validate.mock.calls[0]![0].inputs[0]!;
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publication' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByLabelText('All required labels (one per line)')).toHaveProperty(
    'value',
    'keep-new-draft',
  );
  expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  const current = validate.mock.calls.at(-1)![0].inputs[0]!;
  expect(current.id).toBe(original.id);
  expect(current.expected_version).toBeNull();
  expect(current.definition.connection).toMatchObject({ version: 2, access_version: 3 });
  expect(current.definition.predicate?.all_of).toEqual(['keep-new-draft']);
  expect(publish).toHaveBeenCalledOnce();
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  expect(await within(dialog).findByText('Automatic input published.')).toBeTruthy();
  expect(publish.mock.calls[1]![0].inputs[0]!.expected_version).toBeNull();
  expect(publish.mock.calls[1]![1]['Idempotency-Key']).not.toBe(
    publish.mock.calls[0]![1]['Idempotency-Key'],
  );
  expect(api.list).toHaveBeenCalled();
});

it('refreshes a new group after a publication conflict and lets the operator review renewed member versions', async () => {
  const first = retainedInput().definition;
  const second = { ...first, id: '44444444-4444-4444-8444-444444444444' };
  let stale = false;
  const { validate, publish } = fixture({
    list: vi.fn(async () => ({
      items: [first, second].map((input) => ({
        kind: 'input' as const,
        definition: { ...input, version: stale ? 2 : 1 },
      })),
      next_cursor: null,
    })),
  });
  publish.mockImplementationOnce(async () => {
    stale = true;
    throw new ManagementApiError({
      code: 'precondition_failed',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
      message: 'member access changed',
    });
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add exclusive input group' }));
  for (const member of [first, second])
    await userEvent.click(screen.getByRole('checkbox', { name: new RegExp(`${member.id} · v1`) }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  const original = validate.mock.calls[0]![0].group!;
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publication' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  for (const member of [first, second]) {
    expect(screen.getByRole('checkbox', { name: new RegExp(`${member.id} · v1`) })).toHaveProperty(
      'checked',
      true,
    );
    await userEvent.click(screen.getByRole('checkbox', { name: new RegExp(`${member.id} · v2`) }));
  }
  expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  expect(validate.mock.calls.at(-1)![0].group).toEqual({
    id: original.id,
    expected_version: null,
    members: [
      { id: first.id, version: 2 },
      { id: second.id, version: 2 },
    ],
  });
  expect(publish).toHaveBeenCalledOnce();
});
