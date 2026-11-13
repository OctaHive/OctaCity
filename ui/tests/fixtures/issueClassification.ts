import type { components } from '../../.generated/api/schema';
import { issueFixture, issuePropertyChoicesFixture } from './issues';

export const classificationRun = '44000000-0000-4000-8000-000000000001';
/** Committed values/provenance and independently derived applicability are separate HTTP resources. */
export function issueClassificationFixture(project = 'delivery') {
  const issue = issueFixture(project);
  issue.version = 3;
  issue.applicability_version = 2;
  issue.content.priority = 'critical';
  issue.planning = {
    definition_version: 7,
    component: 'payments',
    type: 'defect',
    size: 'small',
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: { risk: { type: 'choice', value: 'low' } },
  };
  const catalog = issuePropertyChoicesFixture(project);
  catalog.definitions.definition.components = [
    { id: 'payments', name: 'Payments', retired: false },
  ];
  catalog.definitions.definition.priorities = [
    { id: 'critical', name: 'Critical incident', retired: false },
  ];
  catalog.definitions.definition.properties = [
    {
      id: 'risk',
      name: 'Delivery risk',
      retired: false,
      kind: {
        type: 'choice',
        choices: [
          { id: 'low', name: 'Low exposure', retired: false },
          { id: 'high', name: 'High exposure', retired: false },
        ],
      },
    },
  ];
  catalog.definitions.definition.classification = { risk: 'risk' };
  const actor: components['schemas']['AuditActor'] = {
    kind: 'unauthenticated_management',
    identity: null,
  };
  issue.classification = {
    fields: [
      {
        field: { kind: 'priority', definition_version: 7 },
        origin: { kind: 'manual', actor },
        assessed_at: 1700000000000,
        assessed_version: 3,
        requirements_version: 2,
      },
      {
        field: { kind: 'size' },
        origin: {
          kind: 'pipeline',
          actor,
          run_id: classificationRun,
          work_id: '45000000-0000-4000-8000-000000000001',
          record: 'a'.repeat(64),
          policy: 'triage',
          operation_id: 'b'.repeat(64),
        },
        assessed_at: 1700000000000,
        assessed_version: 1,
        requirements_version: 1,
      },
    ],
  };
  const review: components['schemas']['IssueClassificationReview'] = {
    version: 3,
    fields: [
      {
        field: { kind: 'priority', definition_version: 7 },
        problems: [],
        evidence: [],
        requires_verified_evidence: false,
      },
      {
        field: { kind: 'size' },
        problems: [{ code: 'requirements_changed' }],
        evidence: [],
        requires_verified_evidence: false,
      },
    ],
  };
  return { issue, catalog, review };
}
