// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { SignalStep } from './SignalStep';
import { emptyFlowDraft } from './emptyFlowDraft';

afterEach(cleanup);
type S = components['schemas'];
function fixture() {
  const permissions = emptyFlowDraft().limits.permissions;
  const plugin = { identity: 'router', version: 'adapter-v1', digest: 'a'.repeat(64) };
  const schema: S['FactoryFlowDataSchema'] = {
    reference: { identity: 'route-result', version: 'v1', digest: 'b'.repeat(64) },
    shape: {
      type: 'object',
      fields: {
        summary: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 100 } },
      },
    },
  };
  const action: S['FactoryFlowAction'] = { plugin, action: 'route', parameters: {} };
  const outcome: S['FactoryFlowOutcome'] = {
    key: 'analyze',
    kind: 'success',
    schema: schema.reference,
    public_result: true,
    issue_presentation: { kind: 'custom_v1', conclusion_field: 'summary', title: 'CI decision' },
  };
  const node: S['FactoryFlowNodeDefinition'] = {
    key: 'decision',
    subflow: null,
    kind: 'decision_signal',
    required: true,
    action,
    outcomes: [outcome],
    input_schema: schema.reference,
    budget: emptyFlowDraft().root.execution.budget,
    permissions,
  };
  const signal: S['FactoryDecisionSignalAuthoringContract'] = {
    alias: 'route-policy',
    policy: { identity: 'routing-policy', version: 'policy-v2', digest: 'c'.repeat(64) },
    contract: { action, input: schema, outcomes: { analyze: schema } },
    permissions,
  };
  return { node, signal };
}

it('preserves result presentation and disclosure when reapplying the same installed contract', async () => {
  const { node, signal } = fixture();
  const onChange = vi.fn();
  render(<SignalStep node={node} signals={[signal]} onChange={onChange} />);
  await userEvent.click(screen.getByRole('button', { name: 'Use Decision Signal policy' }));
  expect(onChange.mock.calls[0]![0].outcomes).toEqual(node.outcomes);
});

it('shows the exact policy version independently of its adapter version', async () => {
  const { node, signal } = fixture();
  render(<SignalStep node={node} signals={[signal]} onChange={vi.fn()} />);
  expect(screen.getByRole('combobox', { name: 'Decision Signal policy' }).textContent).toBe(
    'route-policy · policy-v2',
  );
});

it('requires fresh display and disclosure settings for a different result contract', async () => {
  const { node, signal } = fixture();
  const replacement: S['FactoryFlowDataSchema'] = {
    reference: { identity: 'private-route', version: 'v2', digest: 'd'.repeat(64) },
    shape: {
      type: 'object',
      fields: {
        private_log: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 100 } },
      },
    },
  };
  signal.contract.outcomes.analyze = replacement;
  const onChange = vi.fn();
  render(<SignalStep node={node} signals={[signal]} onChange={onChange} />);
  await userEvent.click(screen.getByRole('button', { name: 'Use Decision Signal policy' }));
  expect(onChange.mock.calls[0]![0].outcomes).toEqual([
    { key: 'analyze', kind: 'success', schema: replacement.reference },
  ]);
});
