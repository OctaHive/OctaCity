// @vitest-environment jsdom

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';

import { SelectMenu } from './SelectMenu';

afterEach(cleanup);

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
