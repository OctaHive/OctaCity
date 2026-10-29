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
import { SourceInputEditor } from './SourceInputEditor';
import type { components } from '../../../.generated/api/schema';
import { FactoryPipelineInputs } from './FactoryPipelineInputs';
import { PresentationProvider } from '../../app/presentation/PresentationProvider';
import { DEFAULT_PREFERENCES, type Language } from '../../app/presentation/preferences';
import { translate, type MessageKey } from '../../app/presentation/messages';

afterEach(cleanup);
function fixture(
  overrides: Partial<FactorySourcesApi> = {},
  language: Language = 'en',
  onSelect?: (
    intent: SourceIntent,
    choices: components['schemas']['WorkSourceInputChoices'],
  ) => void,
  initialInput?: components['schemas']['WorkSourceInputIntent'],
) {
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
  const queryClient = createConsoleQueryClient();
  render(
    <PresentationProvider
      storage={{
        getItem: () => JSON.stringify({ ...DEFAULT_PREFERENCES, language }),
        setItem: vi.fn(),
      }}
    >
      <QueryClientProvider client={queryClient}>
        <RouterProvider
          router={createMemoryRouter([
            {
              path: '*',
              element: onSelect ? (
                <SourceInputEditor
                  configuration={configuration}
                  api={api}
                  connections={[connection]}
                  repositories={repositories}
                  onRefresh={async () => ({ records: [], connections: [connection] })}
                  onSelect={onSelect}
                />
              ) : (
                <FactoryPipelineInputs
                  configuration={configuration}
                  api={api}
                  repositories={repositories}
                  onManual={() => {}}
                  initialInput={initialInput}
                />
              ),
            },
          ])}
        />
      </QueryClientProvider>
    </PresentationProvider>,
  );
  return { api, validate, publish, connection, choice, read, code, repositories, queryClient };
}
async function choose(label: string, option: string) {
  await userEvent.click(await screen.findByRole('combobox', { name: label }));
  await userEvent.click(await screen.findByRole('option', { name: option }));
}
async function openInput() {
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add automatic input' }));
}

it('distinguishes installed preparation profile versions and revalidates the exact selection without enabling intake', async () => {
  const choices = integrationFixture('WorkSourceInputChoices');
  const original = choices.items[0]!;
  const newer = structuredClone(original);
  newer.binding.configuration.version = 'v2';
  newer.binding.configuration.digest = 'cd'.repeat(32);
  const { connection, validate, publish } = fixture({
    choices: vi.fn(async () => ({ ...choices, items: [original, newer] })),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', original.locator);
  await choose('Selector and input mapping', `${original.binding.configuration.identity} · v1`);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  const review = screen.getByRole('button', { name: 'Review input publication' });
  await waitFor(() => expect(review).toHaveProperty('disabled', false));
  await choose('Selector and input mapping', `${original.binding.configuration.identity} · v2`);
  expect(review).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(validate.mock.calls[1]![0].inputs[0]!.definition.source).toEqual(newer.binding);
  await userEvent.click(review);
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  expect(dialog.textContent).toContain(`${original.binding.configuration.identity} · v2`);
  expect(dialog.textContent).toContain('Automatic intake disabled');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  await within(dialog).findByText('Automatic input published.');
  const submitted = publish.mock.calls[0]![0].inputs[0]!.definition;
  expect(submitted.source).toEqual(newer.binding);
  expect(submitted.target).toEqual(configurationFixture().configuration.reference);
  expect(submitted.enabled).toBe(false);
  expect(submitted.reporter).toBeNull();
  expect(submitted).not.toHaveProperty('issue');
});

it('reviews the exact optional Reporter version and its effects before independently enabling execution intake', async () => {
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  const newer = structuredClone(writer);
  newer.binding.configuration.version = 'v2';
  newer.binding.configuration.digest = 'de'.repeat(32);
  newer.connection_configuration.version = 'v2';
  newer.reporting = {
    mapping: {
      statuses: [{ status: 'inspection-ready', add: ['reviewed'], remove: [], state: null }],
    },
    editable: null,
  };
  const { connection, publish } = fixture({
    choices: vi.fn(async () => ({ ...choices, items: [read, writer, newer] })),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v1`);
  await choose('Code Repository', 'Code in another host · v3');
  await choose('Reporter mapping', `${writer.binding.configuration.identity} · v2`);
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  const review = screen.getByRole('button', { name: 'Review input publication' });
  await waitFor(() => expect(review).toHaveProperty('disabled', false));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Automatic intake enabled' }));
  expect(review).toHaveProperty('disabled', true);
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(review);
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  expect(dialog.textContent).toContain(`${writer.binding.configuration.identity} · v2`);
  expect(dialog.textContent).toContain('inspection-ready');
  expect(dialog.textContent).toContain('Automatic intake enabled');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  await within(dialog).findByText('Automatic input published.');
  const definition = publish.mock.calls[0]![0].inputs[0]!.definition;
  expect(definition.reporter).toEqual(newer.binding);
  expect(definition.connection?.reporter_configuration).toEqual(newer.connection_configuration);
  expect(definition.enabled).toBe(true);
  expect(definition).not.toHaveProperty('issue');
  expect(definition).not.toHaveProperty('import');
});

for (const language of ['en', 'ru'] as const) {
  it(
    'reviews installed preparation and its independent execution target in ' + language,
    async () => {
      const setup = fixture({}, language);
      const text = (key: MessageKey) => translate(language, key);
      const target = configurationFixture().configuration.reference;
      const guidance =
        language === 'en'
          ? 'This profile prepares bounded task and acceptance Artifacts. Missing acceptance blocks execution, not task import.'
          : 'Профиль готовит ограниченные Артефакты задачи и критериев приёмки. Без критериев выполнение заблокировано; импорт задач независим.';
      await userEvent.click(
        screen.getByRole('button', { name: text('factory.configureAutomatic') }),
      );
      await userEvent.click(await screen.findByRole('button', { name: text('factory.addInput') }));
      await choose(text('factory.sourceConnection'), setup.connection.definition.name);
      await choose(text('factory.taskContainer'), setup.read.locator);
      await choose(
        text('factory.inputProfile'),
        `${setup.read.binding.configuration.identity} · v1`,
      );
      expect(screen.getByText(guidance)).toBeTruthy();
      expect(
        screen.getByText(`${text('factory.sourceTarget')}: ${target.id} · v${target.version}`),
      ).toBeTruthy();
      expect(screen.getByRole('button', { name: text('factory.validateInput') })).toHaveProperty(
        'disabled',
        true,
      );
      expect(screen.getByRole('checkbox', { name: text('factory.inputEnabled') })).toHaveProperty(
        'checked',
        false,
      );
      await choose(text('factory.codeRepository'), 'Code in another host · v3');
      await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
      await userEvent.click(screen.getByRole('button', { name: text('factory.reviewInput') }));
      const dialog = await screen.findByRole('dialog', { name: text('factory.publishInput') });
      expect(dialog.textContent).toContain(guidance);
      expect(dialog.textContent).toContain(`${target.id} · v${target.version}`);
      expect(dialog.textContent).toContain('Code in another host · v3');
      expect(setup.publish).not.toHaveBeenCalled();
      expect(screen.queryByRole('textbox', { name: text('factory.sourceTarget') })).toBeNull();
      expect(screen.queryByRole('textbox', { name: 'Credential value' })).toBeNull();
    },
  );
}

it('retries missing Source support without losing the selected connection or claiming an empty result', async () => {
  const choices = vi.fn<FactorySourcesApi['choices']>().mockRejectedValue(
    new ManagementApiError({
      code: 'capability_unavailable',
      status: 422,
      message: 'private-runtime-path',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  const setup = fixture({ choices });
  await openInput();
  await choose('Source connection', setup.connection.definition.name);
  const alert = await screen.findByRole('alert');
  expect(alert.textContent).toContain('installed Source runtime/profile');
  expect(screen.queryByText(/No compatible task-read profile/)).toBeNull();
  choices.mockResolvedValue(setup.choice);
  await userEvent.click(within(alert).getByRole('button', { name: 'Retry' }));
  await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  expect(screen.getByRole('combobox', { name: 'Source connection' }).textContent).toContain(
    setup.connection.definition.name,
  );
  await choose('Task container', setup.read.locator);
  expect(setup.repositories.getRepository).not.toHaveBeenCalled();
  expect(setup.publish).not.toHaveBeenCalled();
});

it.each(['en', 'ru'] as const)(
  'redacts Repository query failures and gives operation-neutral access guidance in %s',
  async (language) => {
    const setup = fixture({}, language);
    const text = (key: MessageKey) => translate(language, key);
    setup.repositories.listRepositories.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'forbidden',
        status: 403,
        message: 'private-repository-token',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
    await userEvent.click(screen.getByRole('button', { name: text('factory.configureAutomatic') }));
    await userEvent.click(await screen.findByRole('button', { name: text('factory.addInput') }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain(
      language === 'en'
        ? 'Check your permissions for this operation and the selected resource'
        : 'Проверьте права на эту операцию и выбранный ресурс',
    );
    expect(alert.textContent).not.toContain(language === 'en' ? 'task-read' : 'чтения задач');
    expect(
      setup.queryClient
        .getQueryCache()
        .getAll()
        .map((query) => String(query.state.error))
        .join(),
    ).not.toContain('private-repository-token');
    await userEvent.click(within(alert).getByRole('button', { name: text('query.retry') }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
    await choose(text('factory.codeRepository'), 'Code in another host · v3');
    expect(setup.publish).not.toHaveBeenCalled();
  },
);

for (const language of ['en', 'ru'] as const) {
  it(
    'explains a missing task-read profile without inventing access or starting intake in ' +
      language,
    async () => {
      const setup = fixture(
        {
          choices: vi.fn(async () => ({
            ...integrationFixture('WorkSourceInputChoices'),
            items: [],
          })),
        },
        language,
      );
      const text = (key: MessageKey) => translate(language, key);
      await userEvent.click(
        screen.getByRole('button', { name: text('factory.configureAutomatic') }),
      );
      await userEvent.click(await screen.findByRole('button', { name: text('factory.addInput') }));
      await choose(text('factory.sourceConnection'), setup.connection.definition.name);
      await screen.findByText(
        language === 'en'
          ? /ask a deployment administrator to install a compatible Source profile/
          : /попросите администратора развёртывания установить совместимый профиль Source/,
      );
      expect(screen.queryByRole('alert')).toBeNull();
      expect(screen.getByRole('button', { name: text('factory.validateInput') })).toHaveProperty(
        'disabled',
        true,
      );
      expect(setup.api.preview).not.toHaveBeenCalled();
      expect(setup.publish).not.toHaveBeenCalled();
    },
  );

  it(
    'explains provider-deferred preview and replaces it after a bounded retry in ' + language,
    async () => {
      const record = retainedInput();
      const preview = vi
        .fn<FactorySourcesApi['preview']>()
        .mockResolvedValueOnce({
          ...integrationFixture('WorkSourcePreview'),
          failure: 'rate_limited',
          retry_after_millis: 1000,
          candidates: [],
        })
        .mockResolvedValue({
          ...integrationFixture('WorkSourcePreview'),
          failure: null,
          candidates: [],
        });
      const setup = fixture(
        { list: vi.fn(async () => ({ items: [record], next_cursor: null })), preview },
        language,
      );
      const text = (key: MessageKey) => translate(language, key);
      await userEvent.click(
        screen.getByRole('button', { name: text('factory.configureAutomatic') }),
      );
      await userEvent.click(await screen.findByRole('button', { name: text('factory.editInput') }));
      await waitFor(() =>
        expect(screen.getByRole('button', { name: text('factory.previewSource') })).toHaveProperty(
          'disabled',
          false,
        ),
      );
      await userEvent.click(screen.getByRole('button', { name: text('factory.previewSource') }));
      const alert = await screen.findByRole('alert');
      expect(alert.textContent).toContain(
        language === 'en'
          ? 'The provider deferred the request. Wait before retrying'
          : 'Провайдер отложил запрос. Подождите перед повтором',
      );
      expect(screen.queryByText(text('factory.previewEmpty'))).toBeNull();
      const retry = within(alert).getByRole('button');
      expect(retry).toHaveProperty('disabled', true);
      expect(preview).toHaveBeenCalledOnce();
      await waitFor(() => expect(retry).toHaveProperty('disabled', false), { timeout: 2000 });
      await userEvent.click(retry);
      await screen.findByText(text('factory.previewEmpty'));
      expect(screen.queryByRole('alert')).toBeNull();
      expect(preview).toHaveBeenCalledTimes(2);
      expect(setup.publish).not.toHaveBeenCalled();
    },
  );

  it(
    'retains the input draft after validation failure and clears the failure on successful retry in ' +
      language,
    async () => {
      const requestId = '0199a6f4-d56c-7440-9aa2-6a320f862795';
      const validate = vi.fn<FactorySourcesApi['validate']>().mockRejectedValue(
        new ManagementApiError({
          code: 'forbidden',
          status: 403,
          message: 'private-provider-token',
          requestId,
          retryAfterMilliseconds: null,
        }),
      );
      const setup = fixture({ validate }, language);
      const text = (key: MessageKey) => translate(language, key);
      await userEvent.click(
        screen.getByRole('button', { name: text('factory.configureAutomatic') }),
      );
      await userEvent.click(await screen.findByRole('button', { name: text('factory.addInput') }));
      await choose(text('factory.sourceConnection'), setup.connection.definition.name);
      await choose(text('factory.taskContainer'), setup.read.locator);
      await choose(
        text('factory.inputProfile'),
        `${setup.read.binding.configuration.identity} · ${setup.read.binding.configuration.version}`,
      );
      await choose(text('factory.codeRepository'), 'Code in another host · v3');
      await userEvent.click(
        screen.getByRole('checkbox', { name: text('factory.customizeFilters') }),
      );
      await userEvent.type(
        screen.getByLabelText(text('factory.allLabels')),
        'transient-selector-draft',
      );
      await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
      const alert = await screen.findByRole('alert');
      expect(alert.textContent).toContain(
        language === 'en'
          ? 'Check your permissions for this operation and the selected resource'
          : 'Проверьте права на эту операцию и выбранный ресурс',
      );
      expect(alert.textContent).toContain(requestId);
      expect(document.body.textContent).not.toContain('private-provider-token');
      expect(screen.getByLabelText(text('factory.allLabels'))).toHaveProperty(
        'value',
        'transient-selector-draft',
      );
      expect(
        JSON.stringify(
          setup.queryClient
            .getQueryCache()
            .getAll()
            .map((query) => query.state),
        ),
      ).not.toContain('transient-selector-draft');
      expect(JSON.stringify({ ...localStorage, ...sessionStorage })).not.toContain(
        'transient-selector-draft',
      );
      validate.mockImplementation(setup.validate);
      await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
      await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
      expect(screen.getByRole('button', { name: text('factory.reviewInput') })).toHaveProperty(
        'disabled',
        false,
      );
      expect(validate.mock.calls[1]![0].inputs[0]!.definition.predicate?.all_of).toEqual([
        'transient-selector-draft',
      ]);
      expect(setup.publish).not.toHaveBeenCalled();
    },
  );

  it.each([
    [
      'invalid_request',
      'Check the selected configuration and supported values',
      'Проверьте выбранные настройки и допустимые значения',
    ],
    [
      'not_found',
      'Refresh the connection and source choices',
      'Обновите подключение и варианты источника',
    ],
    [
      'forbidden',
      'Check your permissions for this operation and the selected resource',
      'Проверьте права на эту операцию и выбранный ресурс',
    ],
    [
      'capability_unavailable',
      'Check connection activation, Project access and the installed Source runtime/profile',
      'Проверьте активацию подключения, доступ проекта и установленный runtime/профиль Source',
    ],
    [
      'conflict',
      'Refresh current settings and review the operation again',
      'Обновите текущие настройки и заново просмотрите операцию',
    ],
    [
      'unavailable',
      'The source is temporarily unavailable. Retry later',
      'Источник временно недоступен. Повторите позже',
    ],
    [
      'rate_limited',
      'The provider deferred the request. Wait before retrying',
      'Провайдер отложил запрос. Подождите перед повтором',
    ],
    [
      'transport_failure',
      'Check the server connection and retry',
      'Проверьте связь с сервером и повторите запрос',
    ],
  ] as const)(
    'shows actionable %s guidance in ' + language + ' and clears it after query recovery',
    async (code, en, ru) => {
      const requestId = '0199a6f4-d56c-7440-9aa2-6a320f862795';
      const failure = new ManagementApiError({
        code,
        status: null,
        message: 'private-provider-token',
        requestId,
        retryAfterMilliseconds: null,
      });
      const list = vi.fn<FactorySourcesApi['list']>().mockRejectedValue(failure);
      fixture({ list }, language);
      await userEvent.click(
        screen.getByRole('button', {
          name: language === 'en' ? 'Configure automatic intake' : 'Настроить автоматический приём',
        }),
      );
      const alert = await screen.findByRole('alert');
      expect(alert.textContent).toContain(language === 'en' ? en : ru);
      expect(alert.textContent).toContain(code);
      expect(alert.textContent).toContain(requestId);
      expect(document.body.textContent).not.toContain('private-provider-token');
      list.mockResolvedValue({ items: [], next_cursor: null });
      await userEvent.click(
        within(alert).getByRole('button', { name: language === 'en' ? 'Retry' : 'Повторить' }),
      );
      await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
      expect(
        screen.getByRole('button', {
          name: language === 'en' ? 'Add automatic input' : 'Добавить автоматический вход',
        }),
      ).toHaveProperty('disabled', false);
    },
  );
}

it('selects only a same-resource granted Reporter and reviews arbitrary installed status effects without claiming verified write readiness', async () => {
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items[0]!;
  const writer = {
    ...read,
    purpose: 'task_update' as const,
    selection: null,
    binding: {
      ...read.binding,
      capability: { ...read.binding.capability, identity: 'writer' },
      configuration: { ...read.binding.configuration, identity: 'editorial-effects' },
    },
    reporting: {
      mapping: {
        statuses: [
          {
            status: 'needs-human-eyes',
            add: ['ready / review'],
            remove: ['needs context'],
            state: null,
          },
        ],
      },
      editable: null,
    },
  };
  const { connection, validate, publish } = fixture({
    choices: vi.fn(async () => ({
      ...choices,
      items: [
        read,
        writer,
        {
          ...writer,
          binding: {
            ...writer.binding,
            resource: { ...writer.binding.resource, identity: 'another-container' },
            configuration: { ...writer.binding.configuration, identity: 'wrong-container' },
          },
        },
      ],
    })),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
  await choose('Code Repository', 'Code in another host · v3');
  await choose('Reporter mapping', 'editorial-effects · v1');
  expect(screen.queryByRole('option', { name: 'wrong-container' })).toBeNull();
  expect(
    screen.getByText(
      'Selecting a Reporter does not verify remote write readiness. Only accepted configured actions can queue updates.',
    ),
  ).toBeTruthy();
  expect(screen.getByText('Unrelated labels are preserved.')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  expect(dialog.textContent).toContain('needs-human-eyes');
  expect(dialog.textContent).toContain('Unrelated labels are preserved.');
  expect(dialog.textContent).toContain(
    'Selecting a Reporter does not verify remote write readiness.',
  );
  expect(dialog.textContent).toContain('ready / review');
  expect(dialog.textContent).toContain('needs context');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  await within(dialog).findByText('Automatic input published.');
  const definition = validate.mock.calls[0]![0].inputs[0]!.definition;
  expect(definition.reporter).toEqual(writer.binding);
  expect(definition.connection?.reporter_configuration).toEqual(writer.connection_configuration);
  expect(publish.mock.calls[0]![0].inputs[0]!.definition.reporting).toBeUndefined();
});

it('edits arbitrary status mappings using only advertised labels and states and invalidates the prior review proof', async () => {
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items[0]!;
  const writer = {
    ...read,
    purpose: 'task_update' as const,
    selection: null,
    binding: {
      ...read.binding,
      configuration: { ...read.binding.configuration, identity: 'editable-effects' },
    },
    reporting: {
      mapping: {
        statuses: [
          { status: 'original', add: ['ready / review'], remove: ['needs context'], state: null },
        ],
      },
      editable: { labels: ['ready / review', 'needs context'], states: ['native-closed'] },
    },
  };
  const { connection, validate } = fixture({
    choices: vi.fn(async () => ({ ...choices, items: [read, writer] })),
  });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
  await choose('Code Repository', 'Code in another host · v3');
  await choose('Reporter mapping', 'editable-effects · v1');
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize status mapping' }));
  const status = screen.getByRole('textbox', { name: 'Logical status' });
  await userEvent.clear(status);
  await userEvent.type(status, 'editorial-approved');
  await choose('Resulting task state', 'native-closed');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(validate.mock.calls[0]![0].inputs[0]!.definition.reporting).toEqual({
    statuses: [
      {
        status: 'editorial-approved',
        add: ['ready / review'],
        remove: ['needs context'],
        state: 'native-closed',
      },
    ],
  });
  const review = screen.getByRole('button', { name: 'Review input publication' });
  await waitFor(() => expect(review).toHaveProperty('disabled', false));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Add label: ready / review' }));
  expect(review).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(review);
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  expect(dialog.textContent).toContain('editorial-approved');
  expect(dialog.textContent).toContain('native-closed');
  expect(dialog.textContent).toContain('needs context');
  expect(screen.queryByRole('textbox', { name: 'Credential value' })).toBeNull();
});
it('selects an authorized task container independently of code, previews read-only and publishes only a freshly validated reviewed input', async () => {
  const { api, validate, publish, connection, read, code } = fixture();
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
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
  const retained = integrationFixture('WorkSourcePreview').candidates[0]!.admissions[0]!;
  expect(screen.getByRole('link', { name: 'Open Run' }).getAttribute('href')).toBe(
    `/factory/${retained.association.run_id}`,
  );
  expect(screen.getByText('Awaiting external confirmation')).toBeTruthy();
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

it('keeps a failed publication classified and retries the same frozen intent without persisting its draft', async () => {
  const record = retainedInput();
  const publish = vi.fn<FactorySourcesApi['publish']>().mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      status: null,
      message: 'private-transport-token',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  const setup = fixture({
    list: vi.fn(async () => ({ items: [record], next_cursor: null })),
    publish,
  });
  publish.mockImplementation(setup.publish);
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  await userEvent.type(
    await screen.findByLabelText('All required labels (one per line)'),
    '-private-draft',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  const alert = await within(dialog).findByRole('alert');
  expect(alert.textContent).toContain('Check the server connection and retry');
  expect(alert.textContent).not.toContain('private-transport-token');
  expect(screen.getByLabelText('All required labels (one per line)')).toHaveProperty(
    'value',
    'retained-label-private-draft',
  );
  expect(JSON.stringify({ ...localStorage, ...sessionStorage })).not.toContain('private-draft');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Retry same command' }));
  await within(dialog).findByText('Automatic input published.');
  expect(within(dialog).queryByRole('alert')).toBeNull();
  expect(publish.mock.calls).toHaveLength(2);
  expect(publish.mock.calls[1]).toEqual(publish.mock.calls[0]);
});

it('refreshes stale validation before retrying while retaining the selector draft', async () => {
  const original = retainedInput();
  let stale = false;
  const validate = vi.fn<FactorySourcesApi['validate']>().mockImplementationOnce(async () => {
    stale = true;
    throw new ManagementApiError({
      code: 'precondition_failed',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
      message: 'private-version-detail',
    });
  });
  const setup = fixture({
    validate,
    list: vi.fn(async () => ({
      items: [{ ...original, definition: { ...original.definition, version: stale ? 2 : 1 } }],
      next_cursor: null,
    })),
  });
  validate.mockImplementation(setup.validate);
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  await userEvent.type(
    await screen.findByLabelText('All required labels (one per line)'),
    '-retained-draft',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await userEvent.click(
    await screen.findByRole('button', { name: 'Refresh current version and access' }),
  );
  await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review input publication' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  expect(validate.mock.calls[1]![0].inputs[0]).toMatchObject({
    expected_version: 2,
    definition: { predicate: { all_of: ['retained-label-retained-draft'] } },
  });
  expect(setup.publish).not.toHaveBeenCalled();
});

it('replaces an earlier deferred preview with the latest classified failure before recovery', async () => {
  const record = retainedInput();
  const preview = vi
    .fn<FactorySourcesApi['preview']>()
    .mockResolvedValueOnce({
      ...integrationFixture('WorkSourcePreview'),
      failure: 'unavailable',
      candidates: [],
    })
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'forbidden',
        status: 403,
        message: 'private-provider-token',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValue({
      ...integrationFixture('WorkSourcePreview'),
      failure: null,
      candidates: [],
    });
  fixture({ list: vi.fn(async () => ({ items: [record], next_cursor: null })), preview });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Preview source (read-only)' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
  await userEvent.click(
    within(await screen.findByRole('alert')).getByRole('button', { name: 'Retry' }),
  );
  await screen.findByText('Error code: forbidden');
  const alerts = screen.getAllByRole('alert');
  expect(alerts).toHaveLength(1);
  expect(alerts[0]!.textContent).toContain(
    'Check your permissions for this operation and the selected resource',
  );
  await userEvent.click(within(alerts[0]!).getByRole('button', { name: 'Retry' }));
  await screen.findByText('No tasks returned by this bounded read.');
  expect(screen.queryByRole('alert')).toBeNull();
});

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
    await choose(
      'Selector and input mapping',
      `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
    );
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
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
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
  await choose('Selector and input mapping', 'second-mapping-same-content · v1');
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(validate.mock.calls[0]![0].inputs[0]!.definition.source.configuration.identity).toBe(
    'second-mapping-same-content',
  );
});

it.each([
  ['required_label_missing', 'A required label is missing'],
  ['labels_present', 'Task has labels; only tasks without labels are eligible'],
] as const)(
  'explains a filtered preview reason %s and distinguishes an unavailable source from empty results',
  async (reason, message) => {
    const preview = integrationFixture('WorkSourcePreview');
    preview.candidates[0]!.disposition = 'filtered';
    preview.candidates[0]!.summary.selections[0]!.reasons = [reason];
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
    await choose(
      'Selector and input mapping',
      `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
    );
    await choose('Code Repository', 'Code in another host · v3');
    await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
    expect(await screen.findByText(new RegExp(message))).toBeTruthy();
    expect(screen.queryByText(new RegExp(reason))).toBeNull();
    unavailable = true;
    await userEvent.click(screen.getByRole('button', { name: 'Preview source (read-only)' }));
    expect(
      await screen.findByText(/Source preview is unavailable; this is not an empty result/),
    ).toBeTruthy();
    expect(screen.queryByText('No tasks returned by this bounded read.')).toBeNull();
  },
);

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
    await choose('Selector and input mapping', 'other-mapping · v1');
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
  expect(screen.getByRole('combobox', { name: 'Reporter mapping' }).textContent).toContain(
    'Retained mapping unavailable: status-mapping',
  );
  await choose('Selector and input mapping', 'another-read-mapping · v1');
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
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
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

it('selects strictly unlabeled tasks independently of exclusions and blocks contradictory required labels', async () => {
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const setup = fixture({
    choices: vi.fn(async () => ({
      ...choices,
      items: [
        {
          ...read,
          predicate_states: ['native-open'],
          predicate_label_presence: ['any', 'none'] satisfies NonNullable<
            typeof read.predicate_label_presence
          >,
        },
      ],
    })),
  });
  await openInput();
  await choose('Source connection', setup.connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v1`);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize filters' }));
  await userEvent.type(screen.getByLabelText('Excluded labels (one per line)'), 'on hold');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(setup.validate.mock.calls[0]![0].inputs[0]!.definition.predicate).toEqual({
    state: null,
    all_of: [],
    any_of: [],
    none_of: ['on hold'],
  });
  await choose('Label presence', 'Only tasks without labels');
  expect(
    screen.queryByText('No label restrictions: tasks without labels are eligible too.'),
  ).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(setup.validate.mock.calls[1]![0].inputs[0]!.definition.predicate).toEqual({
    state: null,
    label_presence: 'none',
    all_of: [],
    any_of: [],
    none_of: ['on hold'],
  });
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Only tasks without labels');
  await userEvent.keyboard('{Escape}');
  await userEvent.type(screen.getByLabelText('All required labels (one per line)'), 'required');
  expect(screen.getByRole('alert').textContent).toContain('cannot require labels');
  expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty('disabled', true);
  expect(screen.getByLabelText('All required labels (one per line)')).toHaveProperty(
    'value',
    'required',
  );
  await choose('Label presence', 'Any labels, including none');
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty('disabled', false);
});

it('preserves saved unlabeled filters across profile changes and refuses unsupported broadening', async () => {
  const original = retainedInput();
  const record = {
    ...original,
    definition: {
      ...original.definition,
      definition: {
        ...original.definition.definition,
        predicate: {
          state: null,
          label_presence: 'none' as const,
          all_of: [],
          any_of: [],
          none_of: ['on hold'],
        },
      },
    },
  };
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const newer = structuredClone(read);
  newer.binding.configuration.version = 'v2';
  newer.binding.configuration.digest = 'cd'.repeat(32);
  const setup = fixture({
    list: vi.fn(async () => ({ items: [record], next_cursor: null })),
    choices: vi.fn(async () => ({
      ...choices,
      items: [
        {
          ...read,
          predicate_states: ['native-open'],
          predicate_label_presence: ['any', 'none'] satisfies NonNullable<
            typeof read.predicate_label_presence
          >,
        },
        { ...newer, predicate_states: ['native-open'] },
      ],
    })),
  });
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit automatic input' }));
  expect(await screen.findByRole('combobox', { name: 'Label presence' })).toHaveProperty(
    'textContent',
    'Only tasks without labels',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(setup.validate.mock.calls[0]![0].inputs[0]!.definition.predicate).toEqual(
    record.definition.definition.predicate,
  );
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v2`);
  expect(screen.getByRole('alert').textContent).toContain('Your filters are retained');
  expect(screen.getByRole('button', { name: 'Validate input' })).toHaveProperty('disabled', true);
  expect(screen.getByLabelText('Excluded labels (one per line)')).toHaveProperty(
    'value',
    'on hold',
  );
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v1`);
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  expect(setup.validate.mock.calls[1]![0].inputs[0]!.definition.predicate).toEqual(
    record.definition.definition.predicate,
  );
});

it.each([
  { language: 'en', choice: 'native-closed', expectedState: 'native-closed' },
  { language: 'ru', choice: 'any', expectedState: null },
] as const)(
  'keeps an incompatible saved state visible and requires an explicit $choice replacement in $language',
  async ({ language, choice, expectedState }) => {
    const text = (key: MessageKey) => translate(language, key);
    const original = retainedInput();
    const record = {
      ...original,
      definition: {
        ...original.definition,
        definition: {
          ...original.definition.definition,
          predicate: {
            state: 'native-open',
            label_presence: 'none' as const,
            all_of: [],
            any_of: [],
            none_of: ['on hold'],
          },
        },
      },
    };
    const choices = integrationFixture('WorkSourceInputChoices', 1);
    const read = choices.items.find((item) => item.purpose === 'task_read')!;
    const replacement = structuredClone(read);
    replacement.binding.configuration.version = 'v2';
    replacement.binding.configuration.digest = 'cd'.repeat(32);
    const setup = fixture(
      {
        list: vi.fn(async () => ({ items: [record], next_cursor: null })),
        choices: vi.fn<FactorySourcesApi['choices']>(async () => ({
          ...choices,
          items: [
            {
              ...read,
              predicate_states: ['native-open'],
              predicate_label_presence: ['any', 'none'],
            },
            {
              ...replacement,
              predicate_states: ['native-closed'],
              predicate_label_presence: ['any', 'none'],
            },
          ],
        })),
      },
      language,
    );
    await userEvent.click(screen.getByRole('button', { name: text('factory.configureAutomatic') }));
    await userEvent.click(await screen.findByRole('button', { name: text('factory.editInput') }));
    await screen.findByRole('combobox', { name: text('factory.labelPresence') });
    await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: text('factory.reviewInput') })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await choose(text('factory.inputProfile'), `${read.binding.configuration.identity} · v2`);
    expect(screen.getByRole('combobox', { name: text('factory.taskState') }).textContent).toBe(
      'native-open',
    );
    expect(screen.getByRole('alert').textContent).toBe(
      language === 'en'
        ? 'Choose a supported state or any state.'
        : 'Выберите допустимое или любое состояние.',
    );
    for (const key of [
      'factory.validateInput',
      'factory.previewSource',
      'factory.reviewInput',
    ] as const) {
      expect(screen.getByRole('button', { name: text(key) })).toHaveProperty('disabled', true);
    }
    await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
    await userEvent.click(screen.getByRole('button', { name: text('factory.previewSource') }));
    expect(setup.validate).toHaveBeenCalledOnce();
    expect(setup.api.preview).not.toHaveBeenCalled();
    expect(setup.publish).not.toHaveBeenCalled();
    await choose(text('factory.taskState'), choice === 'any' ? text('factory.anyState') : choice);
    expect(screen.queryByRole('alert')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: text('factory.previewSource') }));
    await userEvent.click(screen.getByRole('button', { name: text('factory.validateInput') }));
    const expected = {
      state: expectedState,
      label_presence: 'none',
      all_of: [],
      any_of: [],
      none_of: ['on hold'],
    };
    expect(setup.validate.mock.calls[1]![0].inputs[0]!.definition.predicate).toEqual(expected);
    expect(setup.api.preview).toHaveBeenLastCalledWith(
      expect.objectContaining({
        inputs: [
          expect.objectContaining({ definition: expect.objectContaining({ predicate: expected }) }),
        ],
      }),
    );
  },
);

it('reviews the selected installed process completion separately from task-authored acceptance', async () => {
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const project = configurationFixture().configuration.reference.project_id;
  const apiChoices = {
    ...choices,
    items: [
      {
        ...read,
        preparation: {
          kind: 'process_completion' as const,
          value: {
            kind: 'draft' as const,
            project_id: project,
            preset: { identity: 'triage-completion', version: 'v1', digest: 'a'.repeat(64) },
            content: 'Frozen reviewed triage completion',
          },
        },
      },
    ],
  };
  const { connection, publish } = fixture({ choices: vi.fn(async () => apiChoices) });
  await openInput();
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v1`);
  expect(await screen.findByText('Frozen reviewed triage completion')).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Process completion criteria' })).toBeTruthy();
  expect(screen.getByText(/do not replace missing implementation acceptance/)).toBeTruthy();
  expect(screen.getByRole('checkbox', { name: 'Automatic intake enabled' })).toHaveProperty(
    'checked',
    false,
  );
  expect(publish).not.toHaveBeenCalled();
});

it('composes a disabled Source selection for setup without publishing or previewing an uncommitted target', async () => {
  const onSelect = vi.fn();
  const { api, read, connection } = fixture({}, 'en', onSelect);
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose('Selector and input mapping', `${read.binding.configuration.identity} · v1`);
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(
    await screen.findByRole('button', { name: 'Use reviewed Source selection' }),
  );
  expect(onSelect).toHaveBeenCalledWith(
    expect.objectContaining({
      inputs: [
        expect.objectContaining({
          expected_version: null,
          definition: expect.objectContaining({ source: read.binding, enabled: false }),
        }),
      ],
    }),
    expect.objectContaining({ connection_id: connection.id }),
  );
  expect(screen.queryByRole('checkbox', { name: 'Automatic intake enabled' })).toBeNull();
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.preview).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
});

it('publishes the reviewed initial input once and gives a later added input its own identity', async () => {
  const configuration = configurationFixture().configuration.reference;
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const initialInput: components['schemas']['WorkSourceInputIntent'] = {
    id: 'reviewed-input',
    expected_version: null,
    definition: {
      target: configuration,
      source: read.binding,
      reporter: null,
      enabled: false,
      connection: {
        id: choices.connection_id,
        version: choices.connection_version,
        access_version: choices.access_version,
        source_configuration: read.connection_configuration,
        reporter_configuration: null,
      },
      repository: {
        id: '33333333-3333-4333-8333-333333333333',
        project_id: configuration.project_id,
        version: 3,
        definition_digest: 'ab'.repeat(32),
      },
    },
  };
  const { validate, publish, connection } = fixture({}, 'en', undefined, initialInput);
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  const profile = await screen.findByRole('combobox', { name: 'Selector and input mapping' });
  await waitFor(() => expect(profile.textContent).toContain(read.binding.configuration.identity));
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() => expect(validate).toHaveBeenCalled());
  expect(validate.mock.calls[0]![0].inputs[0]).toEqual(
    expect.objectContaining({
      id: 'reviewed-input',
      expected_version: null,
      definition: expect.objectContaining({ target: configuration, enabled: false }),
    }),
  );
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Review input publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish automatic input' });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publication' }));
  await within(dialog).findByText('Automatic input published.');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  await userEvent.click(screen.getByRole('button', { name: 'Close input editor' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add automatic input' }));
  await choose('Source connection', connection.definition.name);
  await choose('Task container', read.locator);
  await choose(
    'Selector and input mapping',
    `${read.binding.configuration.identity} · ${read.binding.configuration.version}`,
  );
  await choose('Code Repository', 'Code in another host · v3');
  await userEvent.click(screen.getByRole('button', { name: 'Validate input' }));
  await waitFor(() => expect(validate).toHaveBeenCalledTimes(2));
  const added = validate.mock.calls[1]![0].inputs[0]!;
  expect(added.id).not.toBe('reviewed-input');
  expect(added.expected_version).toBeNull();
  expect(publish).toHaveBeenCalledOnce();
  await userEvent.click(screen.getByRole('button', { name: 'Close input editor' }));
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  await userEvent.click(screen.getByRole('button', { name: 'Configure automatic intake' }));
  expect(screen.queryByRole('button', { name: 'Validate input' })).toBeNull();
});
