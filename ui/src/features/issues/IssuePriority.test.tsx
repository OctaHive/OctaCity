// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';

afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});

it.each([
  ['Blocker', 3],
  ['High', 2],
  ['Medium', 1],
  ['Low', 0],
] as const)(
  'creates a task with %s priority and displays the same name in its card',
  async (label, priority) => {
    const created = { ...issue, content: { ...issue.content, priority } };
    const { api } = setup('/projects/delivery/issues', (api) => {
      api.create.mockResolvedValue({ disposition: 'applied', resource: created });
      api.get.mockResolvedValue(created);
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Create task' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), issue.content.title);
    const select = screen.getByRole('combobox', { name: 'Priority' });
    expect(screen.queryByRole('spinbutton', { name: 'Priority' })).toBeNull();
    await userEvent.click(select);
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'Not set',
      'Blocker',
      'High',
      'Medium',
      'Low',
    ]);
    await userEvent.click(screen.getByRole('option', { name: label }));
    await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
    const dialog = screen.getByRole('dialog');
    expect(dialog.textContent).toContain(`Priority: ${label}`);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create task' }));
    await waitFor(() =>
      expect(api.create).toHaveBeenCalledWith(
        'delivery',
        { title: issue.content.title, description: '', acceptance: null, priority },
        { 'Idempotency-Key': expect.any(String) },
      ),
    );
    await userEvent.click(within(dialog).getByRole('link', { name: 'Open ISS-42' }));
    await screen.findByRole('heading', { name: `ISS-42 · ${issue.content.title}` });
    const properties = screen.getByRole('complementary', { name: 'Properties' });
    expect(
      within(properties).getByText('Priority', { exact: true }).nextElementSibling?.textContent,
    ).toBe(label);
  },
);

it.each([8, 255])('preserves legacy priority %i through an unrelated edit', async (priority) => {
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue({ ...issue, content: { ...issue.content, priority } });
  });
  await screen.findByRole('heading', { name: `ISS-42 · ${issue.content.title}` });
  const properties = screen.getByRole('complementary', { name: 'Properties' });
  expect(
    within(properties).getByText('Priority', { exact: true }).nextElementSibling?.textContent,
  ).toBe(`Existing priority (${priority})`);
  await userEvent.click(screen.getByRole('button', { name: 'Edit requirements' }));
  const select = screen.getByRole('combobox', { name: 'Priority' });
  expect(select.textContent).toBe(`Existing priority (${priority})`);
  await userEvent.click(select);
  await userEvent.click(screen.getByRole('option', { name: `Existing priority (${priority})` }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Description' }), 'Updated text');
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain(`Priority: Existing priority (${priority})`);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Save task' }));
  await waitFor(() =>
    expect(api.edit).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      { ...issue.content, description: 'Updated text', priority },
      expect.objectContaining({ 'If-Match': '"1"' }),
    ),
  );
});
