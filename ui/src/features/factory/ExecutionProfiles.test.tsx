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

it('reuses a complete published role selection and clears it when an operator switches to a static binding', async () => {
  const { factoryProfileConfigurationFixture } =
    await import('../../../tests/fixtures/factoryRoles');
  const config = factoryProfileConfigurationFixture().configuration;
  const roleNode = config.flow.closure.definitions[0]!.nodes[0]!;
  roleNode.key = 'role_binding';
  const binding = roleNode.profile_selection!.candidates.default!.build;
  const staticFlow = structuredClone(config.flow);
  const staticNode = staticFlow.closure.definitions[0]!.nodes[0]!;
  staticNode.key = 'static_binding';
  staticNode.profile_selection = null;
  staticNode.build = binding;
  const { reference: _reference, ...root } = config.flow.closure.definitions[0]!;
  void _reference;
  const node = { ...roleNode, profile_selection: null };
  const draft = {
    root: { ...root, nodes: [node] },
    data_schemas: config.flow.data_schemas,
    limits: config.flow.limits,
  };
  const choices: import('../../../.generated/api/schema').components['schemas']['FactoryConfigurationChoiceResource'][] =
    [binding.plugin, binding.tool, binding.model_or_tool].map((reference) => ({
      alias: reference.identity,
      identity: reference.identity,
      kind: 'node_capability',
      reference: { kind: 'immutable', reference },
    }));
  choices.push({
    alias: 'build',
    identity: 'build',
    kind: 'build_configuration',
    reference: { kind: 'build_configuration', reference: binding.build_configuration },
  });
  const flows = [config.flow, staticFlow].map((flow) => ({
    flow,
    project_id: config.reference.project_id,
    published_at_unix_ms: 1,
  }));
  const onChange = vi.fn();
  const { rerender } = render(
    <ExecutionProfiles
      value={draft}
      node={node}
      flows={flows}
      choices={choices}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(screen.getByRole('option', { name: `${root.entry} · role_binding · v1` }));
  const selected = onChange.mock.lastCall?.[0];
  expect(selected.root.nodes[0].profile_selection).toEqual(roleNode.profile_selection);
  expect(selected.root.nodes[0].build).toBeNull();
  rerender(
    <ExecutionProfiles
      value={selected}
      node={selected.root.nodes[0]}
      flows={flows}
      choices={choices}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(
    screen.getByRole('option', { name: `${root.entry} · static_binding · v1` }),
  );
  expect(onChange.mock.lastCall?.[0].root.nodes[0].profile_selection).toBeNull();
  expect(onChange.mock.lastCall?.[0].root.nodes[0].build).toEqual(binding);
});
