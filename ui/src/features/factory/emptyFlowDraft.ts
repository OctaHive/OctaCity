import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';

/** A bounded, disabled-by-absence draft with no invented logical nodes or execution authority. */
export function emptyFlowDraft(): components['schemas']['FactoryFlowDefinitionDraft'] {
  const budget = {
    max_attempts: 1,
    max_elapsed_millis: 1,
    max_tokens: 1,
    max_cost_micro_units: 1,
    max_output_bytes: 1,
  };
  const permissions = emptyFactoryPermissions();
  const limits = FACTORY_AUTHORING_SCHEMAS.FactoryFlowAdmissionLimits.properties;
  return {
    root: {
      code_sources: {},
      entry: '',
      nodes: [],
      transitions: [],
      terminals: [],
      context_projections: [],
      data_projections: [],
      input_schema: null,
      execution: {
        budget: { ...budget },
        permissions: structuredClone(permissions),
        max_active_nodes: 1,
      },
    },
    data_schemas: [],
    limits: {
      budget,
      permissions,
      max_depth: limits.max_depth.minimum,
      max_expanded_nodes: limits.max_expanded_nodes.minimum,
      max_fan_out: limits.max_fan_out.minimum,
      max_repeat_count: limits.max_repeat_count.minimum,
      max_wip: limits.max_wip.minimum,
    },
  };
}

/** An explicit empty request, reused for Flow ceilings and new steps; availability grants no access. */
export function emptyFactoryPermissions(): components['schemas']['FactoryPermissions'] {
  return {
    plugins: [],
    executables: [],
    tools: [],
    commands: [],
    max_descendants: 0,
    mounts: [],
    network_hosts: [],
    secret_profiles: [],
    workload_identity_profiles: [],
    resources: {
      cpu_millis: 0,
      disk_bytes: 0,
      elapsed_millis: 0,
      memory_bytes: 0,
      process_count: 0,
    },
    outputs: {
      kinds: [],
      max_artifact_bytes: 0,
      max_artifact_count: 0,
      max_report_bytes: 0,
      max_report_count: 0,
    },
  };
}
