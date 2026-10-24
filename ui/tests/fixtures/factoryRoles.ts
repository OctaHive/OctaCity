import type { components } from '../../.generated/api/schema';
import { configurationFixture } from './factory';

export function factoryRoleFixture() {
  const config = configurationFixture().configuration;
  const exact = (identity: string) => ({ identity, version: 'v1', digest: '9'.repeat(64) });
  const reference = {
    project_id: config.reference.project_id,
    id: '61000000-0000-4000-8000-000000000001',
    version: 1,
    digest: '6'.repeat(64),
  };
  const role: components['schemas']['FactoryRole'] = {
    reference,
    definition: {
      name: 'Code specialist',
      icon: 'bot',
      description: 'Project implementation',
      concurrency_limit: 2,
      instructions: ['Follow Project rules.', 'Review changes.'],
    },
  };
  const profile: components['schemas']['FactoryRoleProfile'] = {
    reference: { ...reference, id: '62000000-0000-4000-8000-000000000001', digest: '7'.repeat(64) },
    role: reference,
    definition: {
      name: 'default',
      instructions: ['Use selected skills.'],
      toolchain: {
        plugin: exact('test.runtime'),
        executable: exact('test.harness'),
        model: exact('test.model'),
      },
      model_parameters: { deliberation: 'balanced' },
      thinking_parameter: 'deliberation',
      execution: {
        environment: { LANGUAGE: { kind: 'public', value: 'en' } },
        cli_arguments: ['--structured'],
        mcp_servers: {},
        integrations: {},
      },
      skills: {},
      context_requirements: [],
      build_configuration: config.reference,
      credential_purpose: 'model',
      permissions: config.flow.limits.permissions,
      budget: { ...config.hard_budget, max_attempts: 2 },
    },
  };
  const parameters: components['schemas']['FactoryFlowDataSchema'] = {
    reference: { identity: 'runtime-parameters', version: 'v1', digest: '8'.repeat(64) },
    shape: {
      type: 'object',
      fields: {
        deliberation: { required: true, value: { type: 'enum', values: ['balanced', 'deep'] } },
      },
    },
  };
  const capability: components['schemas']['FactoryRoleProfileCapabilityDetails'] = {
    definition: profile.definition,
    parameters,
    public_environment: {
      LANGUAGE: {
        reference: parameters.reference,
        shape: { type: 'string', min_bytes: 0, max_bytes: 32 },
      },
    },
  };
  const choice: components['schemas']['FactoryRoleProfileChoice'] = {
    alias: 'installed-runtime',
    toolchain: profile.definition.toolchain,
    build_configuration: profile.definition.build_configuration,
    parameters,
    public_environment: capability.public_environment,
    secret_environment_names: [],
    skills: {},
    mcp_servers: {},
    integrations: {},
    credential_purpose: 'model',
    tools: [],
    budget: profile.definition.budget,
  };
  return { role, profile, capability, choice };
}

export function factoryProfileConfigurationFixture() {
  const details = configurationFixture();
  const { role, profile } = factoryRoleFixture();
  const root = details.configuration.flow.closure.definitions[0]!;
  const node = root.nodes.find((item) => item.key === root.entry)!;
  root.nodes = [node, ...root.nodes.filter((item) => item !== node)];
  node.kind = 'reasoning';
  node.action = null;
  node.build = {
    build_configuration: profile.definition.build_configuration,
    plugin: profile.definition.toolchain.plugin,
    tool: profile.definition.toolchain.executable,
    model_or_tool: profile.definition.toolchain.model,
    result_output: 'result.json',
    result_output_kind: 'report',
    result_outcome: node.outcomes.find((item) => item.kind === 'success')!.key,
    task_digest: '1'.repeat(64),
  };
  node.profile_selection = {
    role: role.reference,
    candidates: { default: { profile, build: node.build } },
    policy: { mode: 'manual', candidate: null },
    failure: { kind: 'manual_selection' },
    selection_budget: {
      attempts: 0,
      elapsed_millis: 0,
      tokens: 0,
      cost_micro_units: 0,
      output_bytes: 0,
    },
  };
  node.build = null;
  return details;
}

export function pendingProfileFixture(): components['schemas']['FactoryProfileSelectionProjection'] {
  const { role, profile } = factoryRoleFixture();
  return {
    id: '2'.repeat(64),
    input_digest: '3'.repeat(64),
    flow_run_id: 'flow-a',
    cycle_id: 'cycle-a',
    node: 'implement',
    generation: 1,
    started_at: 1,
    deadline: 4_000_000_000_000,
    role: role.reference,
    method: 'manual',
    candidate: null,
    profile: null,
    runtime: null,
    model: null,
    thinking_level: null,
    rules: null,
    selector_error: null,
    recovery: null,
    policy: null,
    signal: null,
    predecessor: null,
    control_id: null,
    pending_control: null,
    superseded: false,
    eligible_candidates: [{ candidate: 'default', profile: profile.reference }],
    builds: [],
  };
}
