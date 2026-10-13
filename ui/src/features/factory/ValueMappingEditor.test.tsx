// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, it, expect } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { ValueMappingEditor } from './ValueMappingEditor';
afterEach(cleanup);
it('can extend a published mapping array beyond 64 items within the server contract', async () => {
  function Harness() {
    const [value, setValue] = useState<components['schemas']['FactoryFlowValueMapping']>({
      mapping: 'array',
      items: Array.from({ length: 64 }, () => ({ mapping: 'constant', value: true })),
    });
    return <ValueMappingEditor value={value} onChange={setValue} />;
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add item' }));
  expect(screen.getAllByRole('checkbox', { name: 'Constant value' })).toHaveLength(64);
  expect(screen.queryByRole('textbox', { name: 'Selection path' })).not.toBeNull();
});
it('builds an object with a typed source selection through bounded mapping controls', async () => {
  let observed: components['schemas']['FactoryFlowValueMapping'] = { mapping: 'pointer', path: '' };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <ValueMappingEditor
        value={value}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('combobox', { name: 'Mapping kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Object' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add mapped field' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Mapped field name' }), 'acceptance');
  await userEvent.type(screen.getByRole('textbox', { name: 'Selection path' }), '/sources/task');
  expect(observed).toEqual({
    mapping: 'object',
    fields: { acceptance: { mapping: 'pointer', path: '/sources/task' } },
  });
  expect(screen.queryByRole('textbox', { name: 'Raw JSON' })).toBeNull();
});

it('renders leaf controls for a mapping within the supported server depth', () => {
  let mapping: components['schemas']['FactoryFlowValueMapping'] = {
    mapping: 'pointer',
    path: '/sources/deep/value',
  };
  for (let index = 0; index < 9; index++)
    mapping = { mapping: 'object', fields: { nested: mapping } };
  render(<ValueMappingEditor value={mapping} onChange={() => {}} />);
  expect(screen.queryByRole('textbox', { name: 'Selection path' })).not.toBeNull();
});
