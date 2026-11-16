// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, act } from '@testing-library/react';
import { QueryClientProvider } from '@tanstack/react-query';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryExample } from './FactoryExample';
import { factoryApi } from './api';
type S = components['schemas'];
afterEach(cleanup);
it('opens the ordinary draft immediately after choosing an example without a setup form', async () => {
  const user = userEvent.setup();
  const base = configurationFixture().configuration.flow;
  const { reference: _reference, ...root } = base.closure.definitions[0]!;
  void _reference;
  const draft = {
    root: { ...root, name: 'Ready to edit' },
    limits: base.limits,
    data_schemas: base.data_schemas,
  };
  const preset: S['FactoryPresetSummary'] = {
    reference: { identity: 'analysis', version: 'v1', digest: 'a'.repeat(64) },
    parameters: [],
    profile_aliases: ['assess'],
    required_node_kinds: ['reasoning'],
    editor: { name: 'Analysis', budget: base.limits.budget, max_active_nodes: 1, steps: [] },
  };
  const openPresetDraft = vi.fn(async () => draft);
  const api = {
    ...factoryApi,
    openPresetDraft,
    listPresets: vi.fn(async () => ({ items: [preset], total: 1, next_cursor: null })),
    resolvePreset: vi.fn(),
    getAuthoringCapabilities: vi.fn(),
  };
  const onOpen = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FactoryExample api={api} projectId="project" flowId="new-flow" onOpen={onOpen} />
    </QueryClientProvider>,
  );
  await user.click(await screen.findByRole('button', { name: 'Analysis · v1' }));
  await waitFor(() => expect(onOpen).toHaveBeenCalledWith(draft));
  expect(screen.queryByRole('button', { name: 'Open example draft' })).toBeNull();
  expect(api.resolvePreset).not.toHaveBeenCalled();
  expect(api.getAuthoringCapabilities).not.toHaveBeenCalled();
});

it('keeps a late example response from replacing the more recently selected draft', async () => {
  const user = userEvent.setup();
  const base = configurationFixture().configuration.flow;
  const { reference: _reference, ...root } = base.closure.definitions[0]!;
  void _reference;
  const draft = { root, limits: base.limits, data_schemas: base.data_schemas };
  const presets = ['First', 'Second'].map((name): S['FactoryPresetSummary'] => ({
    reference: { identity: name.toLowerCase(), version: 'v1', digest: 'a'.repeat(64) },
    parameters: [],
    profile_aliases: [],
    required_node_kinds: [],
    editor: { name, budget: base.limits.budget, max_active_nodes: 1, steps: [] },
  }));
  let finish!: (value: typeof draft) => void;
  const pending = new Promise<typeof draft>((resolve) => {
    finish = resolve;
  });
  const openPresetDraft = vi.fn().mockReturnValueOnce(pending).mockResolvedValueOnce(draft);
  const api = {
    ...factoryApi,
    openPresetDraft,
    listPresets: vi.fn(async () => ({ items: presets, total: 2, next_cursor: null })),
  };
  const onOpen = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FactoryExample api={api} projectId="project" flowId="new-flow" onOpen={onOpen} />
    </QueryClientProvider>,
  );
  await user.click(await screen.findByRole('button', { name: 'First · v1' }));
  await user.click(screen.getByRole('button', { name: 'Second · v1' }));
  await waitFor(() => expect(onOpen).toHaveBeenCalledTimes(1));
  await act(async () => finish({ ...draft, root: { ...root, name: 'Obsolete' } }));
  expect(onOpen).toHaveBeenCalledTimes(1);
  expect(openPresetDraft.mock.calls[0]?.[2].aborted).toBe(true);
});
