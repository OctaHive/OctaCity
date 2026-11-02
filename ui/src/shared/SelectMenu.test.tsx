// @vitest-environment jsdom

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { useState } from 'react';

import { SelectMenu } from './SelectMenu';

afterEach(cleanup);

it('keeps an optional search inside the open menu and supports keyboard selection and cancellation', async () => {
  const change = vi.fn();
  function Harness() {
    const [query, setQuery] = useState('');
    return (
      <SelectMenu
        ariaLabel="Version"
        value="205"
        onValueChange={change}
        search={{ label: 'Find version', value: query, onChange: setQuery }}
        options={[
          { value: '205', label: 'Version 205' },
          { value: '1', label: 'Version 1' },
        ]}
      />
    );
  }
  render(<Harness />);
  expect(screen.queryByRole('searchbox')).toBeNull();
  const trigger = screen.getByRole('combobox', { name: 'Version' });
  await userEvent.click(trigger);
  const search = screen.getByRole('searchbox', { name: 'Find version' });
  expect(document.activeElement).toBe(search);
  await userEvent.type(search, '1');
  expect(search).toHaveProperty('value', '1');
  await userEvent.keyboard('{ArrowUp}{Enter}');
  expect(change).toHaveBeenCalledWith('1');
  expect(document.activeElement).toBe(trigger);
  await userEvent.click(trigger);
  await userEvent.keyboard('{Escape}');
  expect(screen.queryByRole('searchbox')).toBeNull();
  expect(change).toHaveBeenCalledTimes(1);
});

it('selects with the keyboard and restores focus', async () => {
  const user = userEvent.setup();
  const onValueChange = vi.fn();
  render(
    <SelectMenu
      ariaLabel="Theme"
      onValueChange={onValueChange}
      options={[
        { label: 'Light', value: 'light' },
        { label: 'Dark', value: 'dark' },
      ]}
      value="light"
    />,
  );

  const trigger = screen.getByRole('combobox', { name: 'Theme' });
  trigger.focus();
  await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');

  expect(onValueChange).toHaveBeenCalledWith('dark');
  expect(screen.queryByRole('listbox', { name: 'Theme' })).toBeNull();
  expect(document.activeElement).toBe(trigger);
});

it('closes without changing the value after an outside interaction', async () => {
  const user = userEvent.setup();
  const onValueChange = vi.fn();
  render(
    <>
      <SelectMenu
        ariaLabel="Theme"
        onValueChange={onValueChange}
        options={[
          { label: 'Light', value: 'light' },
          { label: 'Dark', value: 'dark' },
        ]}
        value="light"
      />
      <button type="button">Outside</button>
    </>,
  );

  await user.click(screen.getByRole('combobox', { name: 'Theme' }));
  expect(screen.getByRole('listbox', { name: 'Theme' })).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'Outside' }));

  expect(screen.queryByRole('listbox', { name: 'Theme' })).toBeNull();
  expect(onValueChange).not.toHaveBeenCalled();
});
