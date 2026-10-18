import { readFileSync } from 'node:fs';
import type { components } from '../../.generated/api/schema';

/** Shared canonical server policy fixture; no provider or Factory prerequisites. */
export function workflowFixture(
  project_id: string,
): components['schemas']['IssueWorkflowResource'] {
  return {
    id: '20000000-0000-4000-8000-000000000001',
    project_id,
    version: 3,
    published_at_unix_ms: 1700000000000,
    definition: JSON.parse(
      readFileSync(
        new URL(
          '../../../server/api/octacity-server-api-rest/fixtures/v1/issue-workflow-definition.json',
          import.meta.url,
        ),
        'utf8',
      ),
    ),
  };
}
