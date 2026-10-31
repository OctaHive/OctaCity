// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, it, expect, vi } from 'vitest';
import { SchemaEditor } from './SchemaEditor';
import { configurationFixture } from '../../../tests/fixtures/factory';
afterEach(cleanup);
it('authors bounded document-guided component and priority fields without a copied enum and explains structural validation', async () => {
  const normalizeSchema = vi.fn(
    async () => configurationFixture().configuration.flow.data_schemas[0]!,
  );
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  expect(
    screen.getByText(
      'Schema validation checks structure. Document meaning requires declared checks or review.',
    ),
  ).toBeTruthy();
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'assessment.result');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Object' }));
  for (const [index, name] of ['component', 'priority'].entries()) {
    await userEvent.click(screen.getByRole('button', { name: 'Add field' }));
    await userEvent.type(screen.getAllByRole('textbox', { name: 'Field name' })[index]!, name);
    await userEvent.click(screen.getAllByRole('combobox', { name: 'Schema type' })[index + 1]!);
    await userEvent.click(screen.getByRole('option', { name: 'String' }));
  }
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'assessment.result',
    version: 'v1',
    shape: {
      type: 'object',
      fields: {
        component: { required: true, value: { type: 'string', min_bytes: 0, max_bytes: 128 } },
        priority: { required: true, value: { type: 'string', min_bytes: 0, max_bytes: 128 } },
      },
    },
  });
  expect(screen.queryByRole('button', { name: 'Add enum value' })).toBeNull();
});
it('authors a signed 64-bit schema bound without rounding it through a JavaScript number', async () => {
  const normalizeSchema = vi.fn(
    async () => configurationFixture().configuration.flow.data_schemas[0]!,
  );
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Schema name' }),
    'operator.exact-bound',
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Integer' }));
  const maximum = screen.getByLabelText('Maximum', { exact: true });
  await userEvent.clear(maximum);
  await userEvent.type(maximum, '9223372036854775807');
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'operator.exact-bound',
    version: 'v1',
    shape: { type: 'integer', minimum: 0, maximum: '9223372036854775807' },
  });
});
it('authors distinct unit and payload variants without changing their case or sibling shapes', async () => {
  const normalizeSchema = vi.fn(
    async () => configurationFixture().configuration.flow.data_schemas[0]!,
  );
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Schema name' }),
    'operator.alternatives',
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Tagged alternatives' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add variant' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Variant name' }), 'Ready');
  await userEvent.click(screen.getByRole('button', { name: 'Add variant' }));
  await userEvent.type(screen.getAllByRole('textbox', { name: 'Variant name' })[1]!, 'RetryAfter');
  await userEvent.click(screen.getAllByRole('checkbox', { name: 'Variant has a payload' })[1]!);
  await userEvent.click(screen.getAllByRole('combobox', { name: 'Schema type' })[1]!);
  await userEvent.click(screen.getByRole('option', { name: 'Integer' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'operator.alternatives',
    version: 'v1',
    shape: {
      type: 'externally_tagged',
      variants: { Ready: null, RetryAfter: { type: 'integer', minimum: 0, maximum: 1 } },
    },
  });
});
it('keeps the edited enum row and sibling value stable when their initial values are equal', async () => {
  const normalizeSchema = vi.fn(
    async () => configurationFixture().configuration.flow.data_schemas[0]!,
  );
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'stable.enum');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Enum' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  const first = screen.getAllByRole('textbox', { name: 'Enum value' })[0]!;
  await userEvent.type(first, 'accepted');
  expect(screen.getAllByRole('textbox', { name: 'Enum value' })[0]).toBe(first);
  expect(first).toHaveProperty('value', 'accepted');
  expect(screen.getAllByRole('textbox', { name: 'Enum value' })[1]).toHaveProperty('value', '');
});
it('builds a bounded object schema with a required boolean field and uses the server-owned exact reference', async () => {
  const schema = {
    reference: { identity: 'operator.check', version: 'v2', digest: '9'.repeat(64) },
    shape: {
      type: 'object' as const,
      fields: { accepted: { required: true, value: { type: 'boolean' as const } } },
    },
  };
  const normalizeSchema = vi.fn(async () => schema);
  const onAdd = vi.fn();
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={onAdd} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'operator.check');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v2');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Object' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add field' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Field name' }), 'accepted');
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'operator.check',
    version: 'v2',
    shape: schema.shape,
  });
  expect(onAdd).toHaveBeenCalledWith(schema);
  expect(screen.queryByRole('textbox', { name: 'Schema digest' })).toBeNull();
});

it('enters multiple string alternatives through typed enum controls', async () => {
  const schema = {
    reference: { identity: 'operator.status', version: 'v1', digest: '7'.repeat(64) },
    shape: { type: 'enum' as const, values: ['accepted', 'rejected'] },
  };
  const normalizeSchema = vi.fn(async () => schema);
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'operator.status');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Enum' }));
  for (const alternative of ['accepted', 'rejected']) {
    await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
    await userEvent.type(
      screen.getAllByRole('textbox', { name: 'Enum value' }).at(-1)!,
      alternative,
    );
  }
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'operator.status',
    version: 'v1',
    shape: schema.shape,
  });
});

it('does not destroy fields when a schema field name collides', async () => {
  const normalizeSchema = vi.fn(
    async () => configurationFixture().configuration.flow.data_schemas[0]!,
  );
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'review_schema');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Object' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add field' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Field name' }), 'accepted');
  await userEvent.click(screen.getByRole('button', { name: 'Add field' }));
  await userEvent.type(screen.getAllByRole('textbox', { name: 'Field name' })[1]!, 'other');
  await userEvent.clear(screen.getAllByRole('textbox', { name: 'Field name' })[1]!);
  await userEvent.type(screen.getAllByRole('textbox', { name: 'Field name' })[1]!, 'accepted');
  expect(screen.getAllByRole('textbox', { name: 'Field name' })).toHaveLength(2);
});

it('normalizes mixed enum alternatives without converting booleans, integers and null to strings', async () => {
  const schema = {
    reference: { identity: 'operator.mixed', version: 'v1', digest: '8'.repeat(64) },
    shape: { type: 'enum' as const, values: [true, 7, null, 'true'] },
  };
  const normalizeSchema = vi.fn(async () => schema);
  render(<SchemaEditor api={{ normalizeSchema }} projectId="visible-project" onAdd={() => {}} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'operator.mixed');
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('combobox', { name: 'Schema type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Enum' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Enum value type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Boolean' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Enum value' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  await userEvent.click(screen.getAllByRole('combobox', { name: 'Enum value type' })[1]!);
  await userEvent.click(screen.getByRole('option', { name: 'Integer' }));
  await userEvent.clear(screen.getByRole('spinbutton', { name: 'Enum value' }));
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Enum value' }), '7');
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  await userEvent.click(screen.getAllByRole('combobox', { name: 'Enum value type' })[2]!);
  await userEvent.click(screen.getByRole('option', { name: 'Null' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add enum value' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Enum value' }), 'true');
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  expect(normalizeSchema).toHaveBeenCalledWith({
    project_id: 'visible-project',
    identity: 'operator.mixed',
    version: 'v1',
    shape: { type: 'enum', values: [true, 7, null, 'true'] },
  });
});
