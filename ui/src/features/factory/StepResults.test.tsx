// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryApi, type FactoryApi } from './api';
import { StepResults, type ResultDraft } from './StepResults';

afterEach(cleanup);
it('names reusable formats after their producing steps and keeps exact references in optional details', async () => {
  const sample = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const schemas = ['a', 'b'].map((digest) => ({
    reference: { identity: `result.generated-${digest}`, version: '1', digest: digest.repeat(64) },
    shape: { type: 'boolean' as const },
  }));
  const node = {
    ...sample,
    key: 'project_check',
    outcomes: [{ key: 'result', kind: 'success' as const, schema: schemas[0]!.reference }],
  };
  const producer = {
    ...sample,
    key: 'systems_analysis',
    outcomes: [{ key: 'analysis', kind: 'success' as const, schema: schemas[1]!.reference }],
  };
  const onChange = vi.fn();
  const props = { node, nodes: [node, producer], schemas, onChange, onDraftChange: vi.fn() };
  const view = render(<StepResults {...props} />);
  await userEvent.click(screen.getByText('Reuse result format', { exact: true }));
  const selector = screen.getByRole('combobox', { name: 'Outcome schema' });
  expect(selector.textContent).toContain('project_check.result');
  expect(screen.queryByText(/result\.generated-/)).toBeNull();
  await userEvent.click(selector);
  await userEvent.click(screen.getByRole('option', { name: 'systems_analysis.analysis' }));
  expect(onChange).toHaveBeenCalledWith({
    outcomes: [{ ...node.outcomes[0], schema: schemas[1]!.reference }],
  });
  view.rerender(<StepResults {...props} nodes={[node, { ...producer, key: 'renamed_analyst' }]} />);
  await userEvent.click(selector);
  expect(screen.getByRole('option', { name: 'renamed_analyst.analysis' })).toBeTruthy();
  await userEvent.keyboard('{Escape}');
  expect(onChange).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByText('Exact format identifiers', { exact: true }));
  for (const schema of schemas) {
    expect(await screen.findByText(schema.reference.identity, { exact: false })).toBeTruthy();
    expect(screen.getByText(schema.reference.digest, { exact: false })).toBeTruthy();
  }
});

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

it('adds a standard Issue card in one choice and lets a custom card name its conclusion', async () => {
  const flow = configurationFixture().configuration.flow;
  const schema: (typeof flow.data_schemas)[number] = {
    reference: { identity: 'assessment', version: '1', digest: 'a'.repeat(64) },
    shape: {
      type: 'object',
      fields: {
        conclusion: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 200 } },
        decision: { required: true, value: { type: 'enum', values: ['eligible'] } },
        summary: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 200 } },
        private_notes: { required: false, value: { type: 'string', min_bytes: 1, max_bytes: 200 } },
      },
    },
  };
  const original: (typeof flow.closure.definitions)[number]['nodes'][number] = {
    ...flow.closure.definitions[0]!.nodes[0]!,
    outcomes: [{ key: 'assessment', kind: 'success' as const, schema: schema.reference }],
  };
  const onChange = vi.fn();
  const normalizeSchema = vi.fn(async () => schema);
  function Harness() {
    const [node, setNode] = useState(original);
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={node}
        schemas={[schema]}
        draft={draft}
        onDraftChange={setDraft}
        services={{
          projectId: 'project',
          api: { ...factoryApi, normalizeSchema },
        }}
        onChange={(patch) => {
          onChange(patch);
          setNode((current) => ({ ...current, ...patch }));
        }}
      />
    );
  }
  const user = userEvent.setup();
  render(<Harness />);
  const edit = () => user.click(screen.getByRole('button', { name: 'Edit result' }));
  const choose = async (name: string, option: string) => {
    await user.click(screen.getByRole('combobox', { name }));
    await user.click(screen.getByRole('option', { name: option }));
  };
  const save = () => user.click(screen.getByRole('button', { name: 'Save result' }));

  await edit();
  await choose('Show on Issue', 'Triage');
  expect(screen.queryByRole('textbox', { name: 'Card title' })).toBeNull();
  await save();
  expect(onChange).toHaveBeenLastCalledWith({
    outcomes: [{ ...original.outcomes[0], issue_presentation: { kind: 'triage_v1' } }],
  });

  await edit();
  expect(screen.getByRole('combobox', { name: 'Show on Issue' }).textContent).toContain('Triage');
  await choose('Show on Issue', 'Custom');
  await user.type(screen.getByRole('textbox', { name: 'Card title' }), 'Project assessment');
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true);
  await user.click(screen.getByRole('combobox', { name: 'Conclusion field' }));
  expect(screen.queryByRole('option', { name: 'private_notes' })).toBeNull();
  await user.click(screen.getByRole('option', { name: 'summary' }));
  await save();
  expect(onChange).toHaveBeenLastCalledWith({
    outcomes: [
      {
        ...original.outcomes[0],
        issue_presentation: {
          kind: 'custom_v1',
          title: 'Project assessment',
          conclusion_field: 'summary',
        },
      },
    ],
  });

  await edit();
  await choose('Show on Issue', 'Off');
  await save();
  expect(onChange).toHaveBeenLastCalledWith({ outcomes: original.outcomes });
  expect(normalizeSchema).not.toHaveBeenCalled();
});

it('offers only compatible built-in cards even when required field names exist', async () => {
  const shape = {
    type: 'object' as const,
    fields: {
      conclusion: {
        required: true,
        value: { type: 'string' as const, min_bytes: 1, max_bytes: 200 },
      },
      decision: { required: true, value: { type: 'boolean' as const } },
      documents: { required: true, value: { type: 'boolean' as const } },
      verdict: { required: true, value: { type: 'enum' as const, values: ['approved'] } },
      review: { required: true, value: { type: 'string' as const, min_bytes: 64, max_bytes: 64 } },
    },
  };
  const node = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  render(
    <StepResults
      node={node}
      schemas={[]}
      onChange={vi.fn()}
      onDraftChange={vi.fn()}
      draft={{ index: null, name: 'assessment', kind: 'success', shape }}
      services={{ api: factoryApi, projectId: 'project' }}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Show on Issue' }));
  expect(screen.queryByRole('option', { name: 'Triage' })).toBeNull();
  expect(screen.queryByRole('option', { name: 'Specifications' })).toBeNull();
  expect(screen.getByRole('option', { name: 'Code review' })).toBeTruthy();
});
