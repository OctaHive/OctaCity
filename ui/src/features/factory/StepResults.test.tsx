// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryApi, type FactoryApi } from './api';
import { StepResults, type ResultDraft } from './StepResults';

afterEach(cleanup);
it('shows the step result fields and edits their format without replacing an existing format or requiring schema metadata', async () => {
  const flow = configurationFixture().configuration.flow;
  const original = {
    reference: { identity: 'original', version: '1', digest: 'a'.repeat(64) },
    shape: {
      type: 'object' as const,
      fields: {
        summary: {
          required: true,
          value: { type: 'string' as const, min_bytes: 0, max_bytes: 200 },
        },
      },
    },
  };
  let saved: (typeof flow.closure.definitions)[number]['nodes'][number] = {
    ...flow.closure.definitions[0]!.nodes[0]!,
    outcomes: [
      {
        key: 'assessed',
        kind: 'success' as const,
        public_result: false,
        schema: original.reference,
      },
    ],
  };
  let formats = [original] as typeof flow.data_schemas;
  const normalizeSchema = vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
    reference: { identity: body.identity, version: body.version, digest: 'b'.repeat(64) },
    shape: body.shape,
  }));
  function Harness() {
    const [node, setNode] = useState(saved);
    const [schemas, setSchemas] = useState(formats);
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={node}
        schemas={schemas}
        draft={draft}
        onDraftChange={setDraft}
        services={{ projectId: 'project', api: { ...factoryApi, normalizeSchema } }}
        onChange={(patch, added = []) => {
          saved = { ...saved, ...patch };
          formats = [...formats, ...added];
          setNode(saved);
          setSchemas(formats);
        }}
      />
    );
  }
  render(<Harness />);
  const result = screen.getByRole('group', { name: 'assessed' });
  expect(within(result).getByText('summary', { exact: true })).toBeTruthy();
  await userEvent.click(within(result).getByRole('button', { name: 'Edit result' }));
  await userEvent.click(screen.getByText('Result structure and limits'));
  await userEvent.click(screen.getByRole('button', { name: 'Add field' }));
  await userEvent.type(screen.getAllByRole('textbox', { name: 'Field name' })[1]!, 'fits');
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  expect(normalizeSchema.mock.calls[0]?.[0].shape).toEqual({
    type: 'object',
    fields: {
      summary: original.shape.fields.summary,
      fits: { required: true, value: { type: 'boolean' } },
    },
  });
  expect(within(result).getByText('fits', { exact: true })).toBeTruthy();
  expect(saved.outcomes[0]?.schema.digest).toBe('b'.repeat(64));
  expect(saved.outcomes[0]?.public_result).toBe(false);
  expect(formats[0]).toEqual(original);
  expect(screen.queryByRole('textbox', { name: 'Schema name' })).toBeNull();
});

it('retains a failed edit but refuses retry with an empty result name and cancels without changing the step', async () => {
  const flow = configurationFixture().configuration.flow;
  const node = { ...flow.closure.definitions[0]!.nodes[0]!, outcomes: [] };
  const normalizeSchema = vi
    .fn<FactoryApi['normalizeSchema']>()
    .mockRejectedValue(new Error('Unavailable'));
  const onChange = vi.fn();
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={node}
        schemas={[]}
        draft={draft}
        onDraftChange={setDraft}
        services={{ projectId: 'project', api: { ...factoryApi, normalizeSchema } }}
        onChange={onChange}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add result' }));
  await userEvent.click(screen.getAllByText('Technical details and reuse').at(-1)!);
  const name = screen.getByRole('textbox', { name: 'Result key' });
  await userEvent.clear(name);
  await userEvent.type(name, 'assessment');
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  await screen.findByRole('button', { name: 'Retry' });
  expect(name).toHaveProperty('value', 'assessment');
  await userEvent.clear(name);
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
  expect(normalizeSchema).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Cancel result edit' }));
  expect(onChange).not.toHaveBeenCalled();
  expect(screen.queryByRole('textbox', { name: 'Result key' })).toBeNull();
  expect(node.outcomes).toEqual([]);
});

it('cancels a pending result check and ignores its late response', async () => {
  const flow = configurationFixture().configuration.flow;
  const node = { ...flow.closure.definitions[0]!.nodes[0]!, outcomes: [] };
  let finish!: (schema: (typeof flow.data_schemas)[number]) => void;
  const response = new Promise<(typeof flow.data_schemas)[number]>((resolve) => {
    finish = resolve;
  });
  const normalizeSchema = vi.fn<FactoryApi['normalizeSchema']>(async () => response);
  const onChange = vi.fn();
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={node}
        schemas={[]}
        draft={draft}
        onDraftChange={setDraft}
        services={{ projectId: 'project', api: { ...factoryApi, normalizeSchema } }}
        onChange={onChange}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add result' }));
  await userEvent.click(screen.getAllByText('Technical details and reuse').at(-1)!);
  await userEvent.clear(screen.getByRole('textbox', { name: 'Result key' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Result key' }), 'assessment');
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  expect(screen.getByRole('button', { name: 'Cancel result edit' }).matches(':disabled')).toBe(
    false,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Cancel result edit' }));
  expect(screen.getByRole('button', { name: 'Add result' })).toBeTruthy();
  const { act } = await import('@testing-library/react');
  await act(async () => finish(flow.data_schemas[0]!));
  expect(onChange).not.toHaveBeenCalled();
  expect(normalizeSchema.mock.calls[0]?.[1]?.aborted).toBe(true);
});
