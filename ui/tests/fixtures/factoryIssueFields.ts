import type { components } from '../../.generated/api/schema';
type S = components['schemas'];

/** Independent wire fixtures: Project enums are supplied by the authoring owner. */
export function factoryIssueFieldsFixture() {
  const plugin = { identity: 'local-classification', version: 'v1', digest: 'f'.repeat(64) };
  const classification: S['FactoryIssueClassificationFieldSelection'] = {
    definition_version: 7,
    priority: true,
    component: true,
    kind: true,
    size: true,
    properties: { severity: 'impact' },
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
      name: 'Priority',
      required_supported: true,
      shape: { type: 'enum', values: ['urgent', 'routine'] },
    },
    {
      source: { kind: 'component' },
      name: 'Component',
      required_supported: true,
      shape: { type: 'enum', values: ['payments'] },
    },
    {
      source: { kind: 'type' },
      name: 'Type',
      required_supported: true,
      shape: { type: 'enum', values: ['defect', 'feature_request'] },
    },
    {
      source: { kind: 'size' },
      name: 'Size',
      required_supported: true,
      shape: { type: 'enum', values: ['small', 'medium', 'large'] },
    },
    {
      source: { kind: 'property', id: 'environment' },
      property_id: 'environment',
      name: 'Environment',
      required_supported: true,
      shape: { type: 'enum', values: ['test', 'production'] },
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
