import type { components } from '../../.generated/api/schema';

export function issueFixture(project_id = 'delivery'): components['schemas']['IssueResource'] {
  return {
    id: '40000000-0000-4000-8000-000000000001',
    project_id,
    number: 42,
    display_key: 'ISS-42',
    version: 1,
    applicability_version: 1,
    content: { title: 'Investigate checkout', description: '', acceptance: null, priority: null },
    workflow_id: '20000000-0000-4000-8000-000000000001',
    workflow_version: 3,
    status_id: '10000000-0000-4000-8000-000000000001',
    created_at_unix_ms: 1700000000000,
    updated_at_unix_ms: 1700000000000,
  };
}

/** Installed project catalog shared by Properties component and HTTP/browser fixtures. */
export function issuePropertyChoicesFixture(
  project_id = 'delivery',
): components['schemas']['IssuePropertyChoices'] {
  const choices: components['schemas']['IssuePropertyChoices'] = {
    definitions: {
      project_id: 'delivery',
      version: 7,
      published_at_unix_ms: 1,
      definition: {
        labels: [],
        properties: [
          {
            id: 'type',
            name: 'Task type',
            retired: false,
            kind: { type: 'choice', choices: [{ id: 'defect', name: 'Defect', retired: false }] },
          },
          {
            id: 'component',
            name: 'Component',
            retired: false,
            kind: { type: 'text', max_bytes: 8 },
          },
          {
            id: 'size',
            name: 'Size estimate',
            retired: false,
            kind: { type: 'number', minimum: 1, maximum: 8 },
          },
          { id: 'verified', name: 'Verified', retired: false, kind: { type: 'boolean' } },
          { id: 'scheduled', name: 'Scheduled date', retired: false, kind: { type: 'date' } },
        ],
      },
    },
    assignees: { available: false, items: [] },
  };
  choices.definitions.project_id = project_id;
  return choices;
}
