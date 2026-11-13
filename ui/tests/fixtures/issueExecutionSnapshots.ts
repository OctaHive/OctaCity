import type { components } from '../../.generated/api/schema';
import { executionFixture } from './issueExecutions';
import { issueFixture } from './issues';
import { configurationFixture } from './factory';

export function snapshotFixture(
  projectId = 'delivery',
): components['schemas']['IssueExecutionSnapshot'] {
  const execution = executionFixture('admitted', 'request-one', projectId);
  const configuration = configurationFixture().configuration.reference;
  return {
    version: 1,
    applicability_version: 1,
    project_id: execution.intent.project_id,
    issue_id: execution.intent.issue_id,
    target: {
      ...execution.intent.target,
      configuration_id: configuration.id,
      configuration_version: configuration.version,
    },
    title_digest: execution.title_digest,
    description_digest: execution.description_digest,
    acceptance_digest: execution.acceptance_digest,
    input: {
      version: 1,
      definition_version: 0,
      schema: { identity: 'reviewed-issue', version: 'v1', digest: 'e'.repeat(64) },
      fields: {
        priority: { source: { kind: 'priority' }, required: true },
        size: { source: { kind: 'size' }, required: true },
        component: { source: { kind: 'component' }, required: false },
      },
      admission: {
        priority: { field: 'priority', values: { high: 80 } },
        risk: { field: 'size', values: { small: 'low' } },
      },
    },
    values: { priority: 'high', size: 'small' },
    omissions: { component: 'missing' },
    definitions: {},
    attribution: {},
    admission_classification: { priority: 80, risk: 'low', metadata: {} },
  };
}
export function snapshotConfiguration(projectId = 'delivery') {
  const details = configurationFixture();
  details.configuration.reference = { ...details.configuration.reference, project_id: projectId };
  details.configuration.flow.issue_snapshot = snapshotFixture(projectId).input;
  return details;
}

/** Assessment pins remain distinct from the compatible consumer catalog and current Issue. */
export function classifiedSnapshotFixture(projectId = 'delivery') {
  const snapshot = snapshotFixture(projectId);
  snapshot.version = 6;
  snapshot.applicability_version = 3;
  snapshot.input.definition_version = 7;
  snapshot.input.fields.exposure = {
    source: { kind: 'classification', slot: 'risk' },
    required: true,
  };
  snapshot.values.exposure = 'routine';
  const property: components['schemas']['IssuePropertyDefinition'] = {
    id: 'risk_band',
    name: 'Delivery exposure',
    retired: false,
    kind: {
      type: 'choice',
      choices: [{ id: 'routine', name: 'Routine delivery', retired: false }],
    },
  };
  for (const version of [2, 7])
    snapshot.definitions[String(version)] = {
      selected_v1: {
        project_id: projectId,
        version,
        published_at: 1,
        publication_digest: String(version).repeat(64),
        priorities: [{ id: 'high', name: 'High', retired: false }],
        properties: [{ definition: property, classification_slot: 'risk' }],
      },
    };
  const actor = { kind: 'unauthenticated_management', identity: null } as const;
  snapshot.attribution = {
    priority: {
      field: { kind: 'priority', definition_version: 2 },
      origin: { kind: 'manual', actor },
      assessed_at: 1700000000000,
      assessed_version: 4,
      requirements_version: 3,
    },
    exposure: {
      field: { kind: 'property', id: 'risk_band', definition_version: 2 },
      origin: {
        kind: 'pipeline',
        actor,
        run_id: '44000000-0000-4000-8000-000000000001',
        work_id: '45000000-0000-4000-8000-000000000001',
        record: 'a'.repeat(64),
        operation_id: 'b'.repeat(64),
        policy: 'triage',
      },
      assessed_at: 1700000001000,
      assessed_version: 5,
      requirements_version: 3,
      evidence: ['c'.repeat(64)],
    },
  };
  return snapshot;
}

export function materialSnapshotFixture(
  projectId = 'delivery',
): components['schemas']['IssueExecutionSnapshot'] {
  const snapshot = snapshotFixture(projectId);
  const actor = { kind: 'unauthenticated_management', identity: null } as const;
  const reference = {
    kind: 'comment',
    project_id: snapshot.project_id,
    issue_id: snapshot.issue_id,
    comment_id: 'comment-approved',
    body_digest: '1'.repeat(64),
  } as const;
  const issueArtifact = {
    artifact_id: 'issue-document-v1',
    content_digest: '2'.repeat(64),
    encoded_size: 22,
  };
  const projectArtifact = {
    artifact_id: 'shared-document-v4',
    content_digest: '3'.repeat(64),
    encoded_size: 21,
  };
  snapshot.input.materials = {
    context: { required: true, max_bytes: 1000, source: { kind: 'selected', max_entries: 4 } },
    extra: {
      required: false,
      max_bytes: 200,
      source: {
        kind: 'exact',
        references: [
          { kind: 'comment', comment_id: 'expired-comment', body_digest: '4'.repeat(64) },
        ],
      },
    },
  };
  snapshot.values.context = [
    'Approved comment <script>alert(1)</script>',
    'Original task document',
    'Original shared guide',
  ];
  snapshot.omissions.extra = 'unavailable';
  snapshot.materials = {
    context: [
      {
        selection: { reference, selected_at: 2, selected_by: actor },
        source: {
          kind: 'comment',
          comment: {
            id: reference.comment_id,
            project_id: snapshot.project_id,
            issue_id: snapshot.issue_id,
            body_digest: reference.body_digest,
            body_bytes: 42,
            created_at: 1,
            actor,
            sequence: 1,
            parent_id: null,
          },
        },
      },
      {
        selection: {
          reference: {
            kind: 'issue_document',
            project_id: snapshot.project_id,
            issue_id: snapshot.issue_id,
            artifact: issueArtifact,
          },
          selected_at: 2,
          selected_by: actor,
        },
        source: {
          kind: 'issue_document',
          document: {
            project_id: snapshot.project_id,
            issue_id: snapshot.issue_id,
            artifact: issueArtifact,
            name: 'Task investigation',
            actor,
            published_at: 1,
            expires_at: 100000,
          },
        },
      },
      {
        selection: {
          reference: {
            kind: 'project_document',
            project_id: snapshot.project_id,
            artifact: projectArtifact,
          },
          selected_at: 2,
          selected_by: actor,
        },
        source: {
          kind: 'project_document',
          document: {
            project_id: snapshot.project_id,
            artifact: projectArtifact,
            name: 'Shared instructions',
            published_at: 1,
            expires_at: 100000,
          },
        },
      },
    ],
  };
  return snapshot;
}

/** Manually prepared current Issue; no producing Run or assessment is required. */
export function manuallyPreparedIssue(
  projectId = 'delivery',
): components['schemas']['IssueResource'] {
  const issue = issueFixture(projectId);
  issue.content = { ...issue.content, priority: 'high', acceptance: 'Check checkout works' };
  issue.planning = {
    definition_version: 0,
    component: null,
    type: 'defect',
    size: 'small',
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: {},
  };
  return issue;
}
