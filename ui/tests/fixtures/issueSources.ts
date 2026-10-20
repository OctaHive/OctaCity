import type { IssueSourceChoice, IssueSynchronization } from '../../src/features/issues/sourceApi';
import { integrationFixture } from './integrations';
import { issueFixture } from './issues';

export function issueSourceChoice(): IssueSourceChoice {
  const read = integrationFixture('WorkSourceInputChoices').items.find(
    (item) => item.purpose === 'task_read',
  )!;
  return {
    source: read.binding,
    native_resource: read.native_id,
    readable: { states: ['Waiting for native review'], labels: ['needs-human-review'] },
    pull_fields: ['title', 'description', 'status'],
    push_fields: [],
    writer: null,
  };
}
export function issueSourceReview(linked = false): IssueSynchronization {
  const issue = issueFixture();
  const choice = issueSourceChoice();
  return {
    issue_version: issue.version,
    workflow: { id: issue.workflow_id, version: issue.workflow_version },
    field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
    mapping: null,
    mapping_reference: null,
    link: linked
      ? {
          issue_id: issue.id,
          selection: {
            project_id: issue.project_id,
            source: choice.source,
            item: { resource: choice.native_resource, item: 'opaque-7' },
          },
          native_correlation: '01'.repeat(32),
          linked_at: 1700000000000,
        }
      : null,
    observation: linked
      ? {
          enabled: true,
          generation: 1,
          accepted: {
            source: choice.source,
            plugin: choice.source.capability,
            provider_configuration: '02'.repeat(32),
            selection_digest: '03'.repeat(32),
            snapshot_digest: '04'.repeat(32),
            observed_at: 1700000000000,
            expires_at: 1700000060000,
          },
          pending: null,
        }
      : null,
  };
}
