// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { useState } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { projectDocumentsApi } from '../../api/projectDocuments';
import { WorkProjectInputs, type WorkProjectSelection } from './WorkProjectInputs';
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it('withdraws manual input readiness when a selected Project document expires without losing the selection', async () => {
  vi.useFakeTimers();
  const document = {
    project_id: 'project',
    name: 'Task',
    artifact: { artifact_id: 'task', content_digest: 'a'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60000,
  };
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [document], next_cursor: null });
  const ready = vi.fn();
  const change = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <WorkProjectInputs
        projectId="project"
        value={{ task: document, acceptance: document, specifications: [] }}
        onChange={change}
        onValidityChange={ready}
      />
    </QueryClientProvider>,
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10);
  });
  expect(ready).toHaveBeenLastCalledWith(true);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60001);
  });
  expect(ready).toHaveBeenLastCalledWith(false);
  expect(change).not.toHaveBeenCalled();
});

it('removes an expired optional input without losing the task, criteria or other documents', async () => {
  const { ready, selection, initial } = setupUnavailableSpecification();
  const checkbox = await screen.findByRole('checkbox', { name: 'Unavailable guidance' });
  expect(ready).toHaveBeenLastCalledWith(false);
  await userEvent.click(checkbox);
  expect(selection()).toEqual({ ...initial, specifications: [initial.specifications[1]] });
  expect(ready).toHaveBeenLastCalledWith(true);
});

it('removes an optional input missing from the catalogue while preserving the remaining manual draft', async () => {
  const { ready, selection, initial } = setupUnavailableSpecification(false);
  await screen.findByRole('checkbox', { name: 'Retained guidance' });
  expect(ready).toHaveBeenLastCalledWith(false);
  await userEvent.click(
    screen.getByRole('button', { name: 'Remove document Unavailable guidance' }),
  );
  expect(selection()).toEqual({ ...initial, specifications: [initial.specifications[1]] });
  expect(ready).toHaveBeenLastCalledWith(true);
});

function setupUnavailableSpecification(availableInCatalog = true) {
  const task = {
    project_id: 'project',
    name: 'Task',
    artifact: { artifact_id: 'task', content_digest: 'a'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60000,
  };
  const unavailable = {
    ...task,
    name: 'Unavailable guidance',
    artifact: { ...task.artifact, artifact_id: 'unavailable' },
    expires_at: Date.now() - 1000,
  };
  const retained = {
    ...task,
    name: 'Retained guidance',
    artifact: { ...task.artifact, artifact_id: 'retained' },
  };
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: availableInCatalog ? [task, unavailable, retained] : [task, retained],
    next_cursor: null,
  });
  const ready = vi.fn();
  const initial: WorkProjectSelection = {
    task,
    acceptance: task,
    specifications: [unavailable, retained],
  };
  let selection = initial;
  function Harness() {
    const [value, setValue] = useState(initial);
    selection = value;
    return (
      <WorkProjectInputs
        projectId="project"
        value={value}
        onChange={setValue}
        onValidityChange={ready}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  return { ready, initial, selection: () => selection };
}
