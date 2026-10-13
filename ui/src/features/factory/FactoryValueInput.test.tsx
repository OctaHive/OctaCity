// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import {
  FactoryValueInput,
  initialFactoryValue,
  validFactoryValue,
  type FactoryValueSchema,
} from './FactoryValueInput';
afterEach(cleanup);

it('edits one bounded object contract through typed fields, optional arrays and finite scalar choices', async () => {
  const schema: FactoryValueSchema = {
    type: 'object',
    fields: {
      approved: { required: true, value: { type: 'boolean' } },
      count: { required: true, value: { type: 'integer', minimum: 1, maximum: 3 } },
      name: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 4 } },
      choice: { required: true, value: { type: 'enum', values: [false, 3, 'yes', null] } },
      tags: {
        required: false,
        value: {
          type: 'array',
          min_items: 0,
          max_items: 1,
          item: { type: 'string', min_bytes: 1, max_bytes: 4 },
        },
      },
    },
  };
  const submitted = vi.fn();
  function Form() {
    const [value, setValue] = useState(() => initialFactoryValue(schema));
    return (
      <>
        <FactoryValueInput label="Response" schema={schema} value={value} onChange={setValue} />
        <button
          type="button"
          disabled={!validFactoryValue(schema, value)}
          onClick={() => submitted(value)}
        >
          Submit observation
        </button>
      </>
    );
  }
  render(<Form />);
  expect(screen.getByRole('button', { name: 'Submit observation' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Response · name' }), 'éé');
  await userEvent.click(screen.getByRole('combobox', { name: 'Response · choice' }));
  await userEvent.click(screen.getByRole('option', { name: 'number: 3' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Include Response · tags' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add observation item' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Response · tags · 1' }), 'tag');
  expect(screen.getByRole('button', { name: 'Add observation item' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Submit observation' }));
  expect(submitted.mock.calls[0]?.[0]).toEqual({
    approved: false,
    count: 1,
    name: 'éé',
    choice: 3,
    tags: ['tag'],
  });
  await userEvent.type(screen.getByRole('textbox', { name: 'Response · name' }), 'x');
  expect(screen.getByRole('button', { name: 'Submit observation' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.queryByRole('textbox', { name: /JSON/ })).toBeNull();
});
