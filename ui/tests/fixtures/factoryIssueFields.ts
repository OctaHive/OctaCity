import type { components } from '../../.generated/api/schema';
type S = components['schemas'];

/** Independent wire fixtures: Project enums are supplied by the authoring owner. */
export function factoryIssueFieldsFixture() {
  const plugin = { identity: 'local-classification', version: 'v1', digest: 'f'.repeat(64) };
  const classification: S['FactoryIssueFieldForm'] = {
    project_id: '11111111-1111-4111-8111-111111111111',
    definition_version: 7,
    fields: [
      { field: { kind: 'priority' }, required: true },
      { field: { kind: 'component' }, required: true },
      { field: { kind: 'type' }, required: true },
      { field: { kind: 'size' }, required: true },
      { field: { kind: 'property', id: 'environment' }, required: true },
      { field: { kind: 'property', id: 'impact' }, required: true },
    ],
  };
  const fields: S['FactoryIssueFieldChoice'][] = [
    {
      source: { kind: 'title' },
      name: 'Title',
      required_supported: true,
      shape: { type: 'string', min_bytes: 1, max_bytes: 512 },
    },
    {
      source: { kind: 'priority' },
      write_target: { kind: 'priority' },
      value_key: 'priority',
      name: 'Priority',
      required_supported: true,
      shape: { type: 'enum', values: ['urgent', 'routine'] },
    },
    {
      source: { kind: 'component' },
      write_target: { kind: 'component' },
      value_key: 'component',
      name: 'Component',
      required_supported: true,
      shape: { type: 'enum', values: ['payments'] },
    },
    {
      source: { kind: 'type' },
      write_target: { kind: 'type' },
      value_key: 'type',
      name: 'Type',
      required_supported: true,
      shape: { type: 'enum', values: ['defect', 'feature_request'] },
    },
    {
      source: { kind: 'size' },
      write_target: { kind: 'size' },
      value_key: 'size',
      name: 'Size',
      required_supported: true,
      shape: { type: 'enum', values: ['small', 'medium', 'large'] },
    },
    {
      source: { kind: 'property', id: 'environment' },
      write_target: { kind: 'property', id: 'environment' },
      value_key: 'property.environment',
      property_id: 'environment',
      name: 'Environment',
      required_supported: true,
      shape: { type: 'enum', values: ['test', 'production'] },
    },
    {
      source: { kind: 'property', id: 'impact' },
      write_target: { kind: 'property', id: 'impact' },
      value_key: 'property.impact',
      property_id: 'impact',
      name: 'Impact',
      required_supported: true,
      shape: { type: 'enum', values: ['major', 'minor'] },
    },
    {
      source: { kind: 'classification', slot: 'severity' },
      property_id: 'impact',
      name: 'Impact',
      required_supported: true,
      shape: { type: 'enum', values: ['major', 'minor'] },
    },
  ];
  return {
    commands: [],
    human_gates: [],
    build_resource_modes: ['none'],
    source_free_incompatibility: null,
    deterministic_gate: {
      policy: plugin,
      parameter_contract: 'declarative_gate_v1',
      max_parameter_bytes: 65536,
      resource_modes: ['none'],
    },
    issue_fields: { definition_version: 7, classification, fields },
    issue_classification: {
      plugin,
      policy: 'project-triage',
      priority_maximum: 100,
      size_labels: [],
      receipt_output_bytes: 317,
      receipt_schema: {
        reference: { identity: 'classification-receipt', version: 'v1', digest: 'b'.repeat(64) },
        shape: {
          type: 'object',
          fields: {
            receipt: { required: true, value: { type: 'string', min_bytes: 64, max_bytes: 64 } },
          },
        },
      },
    },
  } satisfies S['FactoryAuthoringCapabilities'];
}
