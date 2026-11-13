// @vitest-environment jsdom
import { cleanup, screen, within, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { ManagementApiError } from '../../api/client';
import { setup } from '../../../tests/fixtures/issueTestSupport';

afterEach(cleanup);
it('publishes Project component choices separately from custom fields and retains built-in defaults', async () => {
  const { api } = setup('/projects/delivery');
  api.publishProperties.mockImplementation(async (_project, request) => ({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 1,
      published_at_unix_ms: 1,
      definition: request.definition,
    },
  }));
  expect(api.planningChoices).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  await userEvent.type(
    within(components).getByRole('textbox', { name: 'New option name' }),
    'Payments',
  );
  await userEvent.click(within(components).getByRole('button', { name: 'Add option' }));
  expect(api.publishProperties).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Project catalog version 0');
  expect(dialog.textContent).toContain('Payments');
  expect(dialog.textContent).toContain('Existing Issue values and executions are not rewritten.');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  const request = api.publishProperties.mock.calls[0]?.[1];
  expect(request).toMatchObject({
    expected_version: 0,
    definition: {
      priorities: [{ id: 'blocker' }, { id: 'high' }, { id: 'medium' }, { id: 'low' }],
      types: [{ id: 'defect' }, { id: 'feature_request' }],
      components: [{ name: 'Payments', retired: false, id: expect.any(String) }],
      properties: [],
      labels: [],
    },
  });
  expect(api.editPlanning).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
});

it('authors ordinary Project fields with typed bounds without duplicating built-in attributes', async () => {
  const { api } = setup('/projects/delivery');
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const fields = await screen.findByRole('group', { name: 'Custom Issue fields' });
  await userEvent.type(
    within(fields).getByRole('textbox', { name: 'New field name' }),
    'Environment notes',
  );
  await userEvent.click(within(fields).getByRole('combobox', { name: 'Field type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Text' }));
  await userEvent.click(within(fields).getByRole('button', { name: 'Add field' }));
  const field = within(fields).getByRole('group', { name: 'Environment notes' });
  await userEvent.clear(within(field).getByRole('spinbutton', { name: 'Maximum UTF-8 bytes' }));
  await userEvent.type(
    within(field).getByRole('spinbutton', { name: 'Maximum UTF-8 bytes' }),
    '512',
  );
  expect(within(field).getByRole('textbox', { name: 'Field name' })).toHaveProperty(
    'value',
    'Environment notes',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Environment notes');
  expect(dialog.textContent).toContain('512');
  expect(dialog.textContent).toContain('Text');
  api.publishProperties.mockResolvedValue({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 1,
      definition: { labels: [], properties: [] },
      published_at_unix_ms: 1,
    },
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  expect(api.publishProperties.mock.calls[0]?.[1].definition.properties).toEqual([
    {
      id: expect.any(String),
      name: 'Environment notes',
      retired: false,
      kind: { type: 'text', max_bytes: 512 },
    },
  ]);
});

it('publishes choice, boolean, integer and date contracts with one optional choice-field semantic binding', async () => {
  const { api } = setup('/projects/delivery');
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const fields = await screen.findByRole('group', { name: 'Custom Issue fields' });
  async function add(name: string, type: string) {
    await userEvent.type(within(fields).getByRole('textbox', { name: 'New field name' }), name);
    await userEvent.click(within(fields).getByRole('combobox', { name: 'Field type' }));
    await userEvent.click(screen.getByRole('option', { name: type }));
    await userEvent.click(within(fields).getByRole('button', { name: 'Add field' }));
    return within(fields).getByRole('group', { name });
  }
  const risk = await add('Risk band', 'Choice');
  const options = within(risk).getByRole('group', { name: 'Field options' });
  await userEvent.type(
    within(options).getByRole('textbox', { name: 'New option name' }),
    'Normal exposure',
  );
  await userEvent.click(within(options).getByRole('button', { name: 'Add option' }));
  await add('Customer visible', 'Boolean');
  const estimate = await add('Estimate', 'Integer');
  await userEvent.clear(within(estimate).getByRole('spinbutton', { name: 'Maximum integer' }));
  await userEvent.type(within(estimate).getByRole('spinbutton', { name: 'Maximum integer' }), '20');
  await add('Review date', 'Date');
  await userEvent.click(screen.getByRole('combobox', { name: 'Risk field' }));
  expect(screen.queryByRole('option', { name: 'Customer visible' })).toBeNull();
  await userEvent.click(screen.getByRole('option', { name: 'Risk band' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Risk field');
  expect(dialog.textContent).toContain('Normal exposure');
  expect(dialog.textContent).toContain('20');
  api.publishProperties.mockResolvedValue({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 1,
      definition: { labels: [], properties: [] },
      published_at_unix_ms: 1,
    },
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  const definition = api.publishProperties.mock.calls[0]?.[1].definition;
  expect(definition?.properties).toEqual([
    {
      id: expect.any(String),
      name: 'Risk band',
      retired: false,
      kind: {
        type: 'choice',
        choices: [{ id: expect.any(String), name: 'Normal exposure', retired: false }],
      },
    },
    { id: expect.any(String), name: 'Customer visible', retired: false, kind: { type: 'boolean' } },
    {
      id: expect.any(String),
      name: 'Estimate',
      retired: false,
      kind: { type: 'number', minimum: 0, maximum: 20 },
    },
    { id: expect.any(String), name: 'Review date', retired: false, kind: { type: 'date' } },
  ]);
  expect(definition?.classification).toEqual({ risk: definition?.properties[0]?.id });
  expect(api.editPlanning).not.toHaveBeenCalled();
});

it.each([
  ['Text', 'Maximum UTF-8 bytes', '0'],
  ['Integer', 'Maximum integer', '-1'],
  ['Choice', null, null],
])(
  'blocks invalid %s contracts before review without silently correcting their values',
  async (type, control, value) => {
    const { api } = setup('/projects/delivery');
    await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
    const fields = await screen.findByRole('group', { name: 'Custom Issue fields' });
    await userEvent.type(
      within(fields).getByRole('textbox', { name: 'New field name' }),
      'Guarded field',
    );
    await userEvent.click(within(fields).getByRole('combobox', { name: 'Field type' }));
    await userEvent.click(screen.getByRole('option', { name: type }));
    await userEvent.click(within(fields).getByRole('button', { name: 'Add field' }));
    if (control)
      fireEvent.change(within(fields).getByRole('spinbutton', { name: control }), {
        target: { value },
      });
    expect(screen.getByRole('button', { name: 'Review field definitions' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(screen.getByRole('alert').textContent).toContain('Guarded field');
    expect(api.publishProperties).not.toHaveBeenCalled();
  },
);

it('renames and retires existing choices and fields without changing stable identities or unrelated catalog data', async () => {
  const { api } = setup('/projects/delivery', (api) =>
    api.planningChoices.mockResolvedValue({
      definitions: {
        project_id: 'delivery',
        version: 3,
        published_at_unix_ms: 1,
        definition: {
          labels: [{ id: 'tracked', name: 'Tracked', retired: false }],
          components: [{ id: 'payments', name: 'Payments', retired: false }],
          properties: [
            {
              id: 'risk_band',
              name: 'Risk band',
              retired: false,
              kind: {
                type: 'choice',
                choices: [{ id: 'routine', name: 'Routine', retired: false }],
              },
            },
          ],
          classification: { risk: 'risk_band' },
        },
      },
      assignees: { available: false, items: [] },
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  await userEvent.clear(within(components).getByRole('textbox', { name: 'Option display name' }));
  await userEvent.type(
    within(components).getByRole('textbox', { name: 'Option display name' }),
    'Payment service',
  );
  await userEvent.click(within(components).getByRole('checkbox', { name: 'Retired' }));
  const field = screen.getByRole('group', { name: 'Risk band' });
  await userEvent.clear(within(field).getByRole('textbox', { name: 'Field name' }));
  await userEvent.type(
    within(field).getByRole('textbox', { name: 'Field name' }),
    'Delivery exposure',
  );
  await userEvent.click(within(field).getByRole('checkbox', { name: 'Retire field' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Payment service · payments · Retired');
  expect(dialog.textContent).toContain('Delivery exposure · risk_band · Choice · Retired');
  api.publishProperties.mockResolvedValue({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 4,
      definition: { labels: [], properties: [] },
      published_at_unix_ms: 1,
    },
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  expect(api.publishProperties.mock.calls[0]?.[1]).toMatchObject({
    expected_version: 3,
    definition: {
      labels: [{ id: 'tracked', name: 'Tracked', retired: false }],
      components: [{ id: 'payments', name: 'Payment service', retired: true }],
      properties: [
        {
          id: 'risk_band',
          name: 'Delivery exposure',
          retired: true,
          kind: { type: 'choice', choices: [{ id: 'routine', name: 'Routine', retired: false }] },
        },
      ],
      classification: { risk: 'risk_band' },
    },
  });
  expect(api.edit).not.toHaveBeenCalled();
});

it('reconciles a stale publication explicitly while retaining the local addition and another administrators new field', async () => {
  const { api } = setup('/projects/delivery');
  api.publishProperties.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      status: 409,
      message: 'Field catalog changed',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  await userEvent.type(
    within(components).getByRole('textbox', { name: 'New option name' }),
    'Payments',
  );
  await userEvent.click(within(components).getByRole('button', { name: 'Add option' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', {
      name: 'Publish field definitions',
    }),
  );
  await screen.findByText('Field catalog changed');
  api.planningChoices.mockResolvedValue({
    definitions: {
      project_id: 'delivery',
      version: 1,
      published_at_unix_ms: 1,
      definition: {
        labels: [],
        components: [{ id: 'search', name: 'Search', retired: false }],
        properties: [
          {
            id: 'environment',
            name: 'Environment',
            retired: false,
            kind: { type: 'text', max_bytes: 100 },
          },
        ],
      },
    },
    assignees: { available: false, items: [] },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  expect(
    await within(components).findByRole('textbox', { name: 'Option display name' }),
  ).toHaveProperty('value', 'Payments');
  await userEvent.click(screen.getByRole('button', { name: 'Reconcile with current catalog' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Project catalog version 1');
  expect(dialog.textContent).toContain('Environment');
  expect(dialog.textContent).toContain('Search');
  expect(dialog.textContent).toContain('Payments');
  api.publishProperties.mockResolvedValueOnce({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 2,
      definition: { labels: [], properties: [] },
      published_at_unix_ms: 1,
    },
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  const [original, renewed] = api.publishProperties.mock.calls;
  expect(renewed?.[1]).toMatchObject({
    expected_version: 1,
    definition: {
      components: [{ id: 'search', name: 'Search' }, { name: 'Payments' }],
      properties: [{ id: 'environment', name: 'Environment' }],
    },
  });
  expect(original?.[2]['Idempotency-Key']).not.toBe(renewed?.[2]['Idempotency-Key']);
});

it('requires an explicit decision when another administrator renames the same stable component', async () => {
  const initial = {
    project_id: 'delivery',
    version: 1,
    published_at_unix_ms: 1,
    definition: {
      labels: [],
      properties: [],
      components: [{ id: 'payments', name: 'Payments', retired: false }],
    },
  };
  const { api } = setup('/projects/delivery', (api) =>
    api.planningChoices.mockResolvedValue({
      definitions: initial,
      assignees: { available: false, items: [] },
    }),
  );
  api.publishProperties.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      status: 409,
      message: 'Catalog raced',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  const input = within(components).getByRole('textbox', { name: 'Option display name' });
  await userEvent.clear(input);
  await userEvent.type(input, 'Local payments');
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', {
      name: 'Publish field definitions',
    }),
  );
  await screen.findByText('Catalog raced');
  api.planningChoices.mockResolvedValue({
    definitions: {
      ...initial,
      version: 2,
      definition: {
        ...initial.definition,
        components: [{ id: 'payments', name: 'Remote payments', retired: false }],
      },
    },
    assignees: { available: false, items: [] },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  await userEvent.click(screen.getByRole('button', { name: 'Reconcile with current catalog' }));
  expect(screen.getByRole('button', { name: 'Review field definitions' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(
    screen.getByRole('combobox', { name: 'Resolve catalog conflict: components/payments' }),
  );
  await userEvent.click(screen.getByRole('option', { name: 'Use current value' }));
  await userEvent.click(screen.getByRole('button', { name: 'Reconcile with current catalog' }));
  expect(input).toHaveProperty('value', 'Remote payments');
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Project catalog version 2');
  expect(dialog.textContent).toContain('Remote payments · payments');
  expect(dialog.textContent).not.toContain('Local payments');
  expect(api.publishProperties).toHaveBeenCalledTimes(1);
});

it('retains a denied administration draft while Issue editing stays a separate command', async () => {
  const { api } = setup('/projects/delivery');
  api.publishProperties.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Project administration required',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  await userEvent.type(
    within(components).getByRole('textbox', { name: 'New option name' }),
    'Payments',
  );
  await userEvent.click(within(components).getByRole('button', { name: 'Add option' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', {
      name: 'Publish field definitions',
    }),
  );
  await screen.findByText('Project administration required');
  await userEvent.click(screen.getByRole('button', { name: 'Abandon' }));
  expect(within(components).getByRole('textbox', { name: 'Option display name' })).toHaveProperty(
    'value',
    'Payments',
  );
  expect(api.edit).not.toHaveBeenCalled();
  expect(api.editPlanning).not.toHaveBeenCalled();
});

it('keeps a collapsed in-memory draft until explicit discard, without changing Project choices', async () => {
  const { api } = setup('/projects/delivery');
  const toggle = await screen.findByRole('button', { name: 'Issue field settings' });
  await userEvent.click(toggle);
  let components = await screen.findByRole('group', { name: 'Components' });
  await userEvent.type(
    within(components).getByRole('textbox', { name: 'New option name' }),
    'Preserved draft',
  );
  await userEvent.click(within(components).getByRole('button', { name: 'Add option' }));
  await userEvent.click(toggle);
  await userEvent.click(toggle);
  components = screen.getByRole('group', { name: 'Components' });
  expect(within(components).getByRole('textbox', { name: 'Option display name' })).toHaveProperty(
    'value',
    'Preserved draft',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Discard field draft' }));
  expect(
    within(screen.getByRole('group', { name: 'Components' })).queryByRole('textbox', {
      name: 'Option display name',
    }),
  ).toBeNull();
  expect(api.publishProperties).not.toHaveBeenCalled();
});

it('retains concurrently added choice IDs when explicitly keeping a local field rename', async () => {
  const initial = {
    project_id: 'delivery',
    version: 1,
    published_at_unix_ms: 1,
    definition: {
      labels: [],
      properties: [
        {
          id: 'risk_band',
          name: 'Risk band',
          retired: false,
          kind: { type: 'choice', choices: [{ id: 'routine', name: 'Routine', retired: false }] },
        },
      ],
    },
  };
  const { api } = setup('/projects/delivery', (api) =>
    api.planningChoices.mockResolvedValue({
      definitions: initial,
      assignees: { available: false, items: [] },
    }),
  );
  api.publishProperties.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      status: 409,
      message: 'Choice catalog raced',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const field = await screen.findByRole('group', { name: 'Risk band' });
  const input = within(field).getByRole('textbox', { name: 'Field name' });
  await userEvent.clear(input);
  await userEvent.type(input, 'Local delivery risk');
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', {
      name: 'Publish field definitions',
    }),
  );
  await screen.findByText('Choice catalog raced');
  api.planningChoices.mockResolvedValue({
    definitions: {
      ...initial,
      version: 2,
      definition: {
        ...initial.definition,
        properties: [
          {
            id: 'risk_band',
            name: 'Current delivery risk',
            retired: false,
            kind: {
              type: 'choice',
              choices: [
                { id: 'routine', name: 'Routine', retired: false },
                { id: 'elevated', name: 'Elevated', retired: false },
              ],
            },
          },
        ],
      },
    },
    assignees: { available: false, items: [] },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  await userEvent.click(screen.getByRole('button', { name: 'Reconcile with current catalog' }));
  await userEvent.click(
    screen.getByRole('combobox', { name: 'Resolve catalog conflict: properties/risk_band' }),
  );
  await userEvent.click(screen.getByRole('option', { name: 'Keep my change' }));
  await userEvent.click(screen.getByRole('button', { name: 'Reconcile with current catalog' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Local delivery risk · risk_band');
  expect(dialog.textContent).toContain('Elevated · elevated');
  expect(dialog.textContent).toContain('Routine · routine');
});

it('can author a supported semantic option ID independently from its display name', async () => {
  const { api } = setup('/projects/delivery');
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const fields = await screen.findByRole('group', { name: 'Custom Issue fields' });
  await userEvent.type(
    within(fields).getByRole('textbox', { name: 'New field name' }),
    'Reproduction',
  );
  await userEvent.click(within(fields).getByRole('combobox', { name: 'Field type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Choice' }));
  await userEvent.click(within(fields).getByRole('button', { name: 'Add field' }));
  const options = within(fields).getByRole('group', { name: 'Field options' });
  await userEvent.type(
    within(options).getByRole('textbox', { name: 'New option name' }),
    'Неизвестно',
  );
  await userEvent.type(
    within(options).getByRole('textbox', { name: 'New option ID (optional)' }),
    'unknown',
  );
  await userEvent.click(within(options).getByRole('button', { name: 'Add option' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Reproducibility field' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reproduction' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field definitions' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Неизвестно · unknown');
  api.publishProperties.mockResolvedValue({
    disposition: 'applied',
    resource: {
      project_id: 'delivery',
      version: 1,
      definition: { labels: [], properties: [] },
      published_at_unix_ms: 1,
    },
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish field definitions' }));
  await within(dialog).findByText('Field definitions published.');
  expect(api.publishProperties.mock.calls[0]?.[1].definition.properties[0]?.kind).toEqual({
    type: 'choice',
    choices: [{ id: 'unknown', name: 'Неизвестно', retired: false }],
  });
});

it('discards unadded field and option input buffers only on explicit discard', async () => {
  const { api } = setup('/projects/delivery');
  await userEvent.click(await screen.findByRole('button', { name: 'Issue field settings' }));
  const components = await screen.findByRole('group', { name: 'Components' });
  const option = within(components).getByRole('textbox', {
    name: 'New option name',
  });
  await userEvent.type(option, 'Unadded component');
  const fields = screen.getByRole('group', { name: 'Custom Issue fields' });
  await userEvent.type(
    within(fields).getByRole('textbox', {
      name: 'New field name',
    }),
    'Unadded field',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Discard field draft' }));
  expect(
    within(screen.getByRole('group', { name: 'Components' })).getByRole('textbox', {
      name: 'New option name',
    }),
  ).toHaveProperty('value', '');
  expect(
    within(screen.getByRole('group', { name: 'Custom Issue fields' })).getByRole('textbox', {
      name: 'New field name',
    }),
  ).toHaveProperty('value', '');
  expect(api.publishProperties).not.toHaveBeenCalled();
});
