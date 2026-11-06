// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { PublicationStep } from './PublicationStep';

afterEach(cleanup);

it('keeps the Issue card when a compatible publication target replaces the current target', async () => {
  const { configuration } = factoryProfileConfigurationFixture();
  const original = configuration.flow.closure.definitions[0]!.nodes[0]!;
  const plugin = { identity: 'publisher', version: 'v1', digest: 'a'.repeat(64) };
  const target = { ...plugin, identity: 'first-target' };
  const outcome = { ...original.outcomes[0]!, kind: 'success' as const };
  const issuePresentation = { kind: 'publication_v1' as const, slot: 'delivery' };
  const receipt = { reference: outcome.schema, shape: { type: 'boolean' as const } };
  const current = {
    plugin,
    target,
    outcome,
    receipt_schema: receipt,
    review_url: true,
    permissions: original.permissions,
  };
  const replacement = { ...current, target: { ...target, identity: 'second-target' } };
  const onChange = vi.fn();
  render(
    <PublicationStep
      node={{
        ...original,
        action: { plugin, action: 'publish_candidate', parameters: { target } },
        outcomes: [{ ...outcome, issue_presentation: issuePresentation }],
      }}
      choices={[current, replacement]}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Publication target' }));
  await userEvent.click(screen.getByRole('option', { name: 'second-target · v1' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use publication target' }));
  expect(onChange.mock.lastCall?.[0].outcomes).toEqual([
    { ...outcome, issue_presentation: issuePresentation },
  ]);
});

it('routes publication conflicts and rejections to configured failure outcomes', async () => {
  const { configuration } = factoryProfileConfigurationFixture();
  const original = configuration.flow.closure.definitions[0]!.nodes[0]!;
  const plugin = { identity: 'publisher', version: 'v1', digest: 'a'.repeat(64) };
  const target = { ...plugin, identity: 'review-target' };
  const success = { ...original.outcomes[0]!, key: 'published', kind: 'success' as const };
  const node = {
    ...original,
    action: { plugin, action: 'publish_candidate', parameters: { target } },
    outcomes: [
      success,
      { ...success, key: 'conflict', kind: 'failure' as const },
      { ...success, key: 'blocked', kind: 'failure' as const },
    ],
  };
  const onChange = vi.fn();
  render(<PublicationStep node={node} choices={[]} onChange={onChange} />);
  await userEvent.click(screen.getByText('Publication inputs'));
  await userEvent.click(screen.getByRole('combobox', { name: 'On publication conflict' }));
  expect(screen.queryByRole('option', { name: 'published' })).toBeNull();
  await userEvent.click(screen.getByRole('option', { name: 'conflict' }));
  expect(onChange.mock.lastCall?.[0].action.parameters).toEqual({
    target,
    conflict_outcome: 'conflict',
  });
  await userEvent.click(screen.getByRole('combobox', { name: 'On publication rejection' }));
  await userEvent.click(screen.getByRole('option', { name: 'blocked' }));
  expect(onChange.mock.lastCall?.[0].action.parameters).toEqual({
    target,
    rejected_outcome: 'blocked',
  });
});
