// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { ExecutionProfiles } from './ExecutionProfiles';
afterEach(cleanup);
it('offers a server-recognized native gate without a deployment alias and pins its exact binding', async () => {
  const configuration = configurationFixture().configuration;
  const flow = configuration.flow;
  const definition = flow.closure.definitions[0]!;
  const profile = definition.nodes[0]!;
  const policy = {
    identity: 'server.recognized.gate',
    version: 'release-11',
    digest: 'c'.repeat(64),
  };
  profile.kind = 'deterministic_gate';
  profile.action = null;
  profile.gate = { policy, parameters: { rules: [], fallback: profile.outcomes[0]!.key } };
  const node = { ...profile, gate: null };
  const { reference, ...root } = definition;
  void reference;
  const onChange = vi.fn();
  render(
    <ExecutionProfiles
      value={{
        root: { ...root, nodes: [node] },
        data_schemas: flow.data_schemas,
        limits: flow.limits,
      }}
      node={node}
      choices={[]}
      flows={[
        {
          flow,
          project_id: configuration.reference.project_id,
          published_at_unix_ms: 1,
          native_gate_policies: [policy],
        },
      ]}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(
    screen.getByRole('option', {
      name: `${definition.entry} · ${profile.key} · v${definition.reference.version}`,
    }),
  );
  expect(onChange.mock.calls[0]?.[0].root.nodes[0].gate).toEqual({
    policy,
    parameters: { rules: [], fallback: profile.outcomes[0]!.key },
  });
});
it('does not offer a published profile whose evidence tool is missing from the visible resource choices', async () => {
  const configuration = configurationFixture().configuration;
  const flow = configuration.flow;
  const definition = flow.closure.definitions[0]!;
  const node = definition.nodes[0]!;
  const capability = node.action!.plugin;
  const schema = flow.data_schemas[0]!.reference;
  node.kind = 'reasoning';
  node.action = null;
  node.build = {
    build_configuration: {
      id: '467728d6-88ce-4d26-a7ca-4cff47e8e05b',
      project_id: configuration.reference.project_id,
      version: 1,
      definition_digest: '07'.repeat(32),
    },
    plugin: capability,
    tool: capability,
    model_or_tool: capability,
    task_digest: '08'.repeat(32),
    result_outcome: node.outcomes[0]!.key,
    result_output: 'result',
    result_output_kind: 'report',
    evidence: [
      {
        kind: 'custom.evidence',
        output_kind: 'report',
        schema,
        plugin: capability,
        tool: { ...capability, identity: 'private.evidence-tool' },
      },
    ],
  };
  const { reference, ...root } = definition;
  void reference;
  render(
    <ExecutionProfiles
      value={{ root, data_schemas: flow.data_schemas, limits: flow.limits }}
      node={node}
      flows={[{ flow, project_id: configuration.reference.project_id, published_at_unix_ms: 1 }]}
      choices={[
        {
          alias: 'visible.capability',
          identity: capability.identity,
          kind: 'node_capability',
          reference: { kind: 'immutable', reference: capability },
        },
        {
          alias: 'visible.build',
          identity: node.build.build_configuration.id,
          kind: 'build_configuration',
          reference: { kind: 'build_configuration', reference: node.build.build_configuration },
        },
      ]}
      onChange={() => {}}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  expect(
    screen.queryByRole('option', {
      name: `${definition.entry} · ${node.key} · v${definition.reference.version}`,
    }),
  ).toBeNull();
  expect(screen.queryByText('private.evidence-tool')).toBeNull();
});
