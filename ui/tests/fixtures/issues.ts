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
