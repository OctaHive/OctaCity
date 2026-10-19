import type { IssueExecution } from '../../src/features/issues/executionApi';
import { issueFixture } from './issues';

/** Retained metadata only; no task bodies, admission keys or provider diagnostics. */
export function executionFixture(
  state: 'pending' | 'admitted' = 'admitted',
  id = 'request-one',
  projectId = 'delivery',
): IssueExecution {
  return {
    id,
    state,
    requested_at: 1,
    intent: {
      project_id: projectId,
      issue_id: issueFixture(projectId).id,
      expected_version: 1,
      target: {
        configuration_id: 'config',
        configuration_version: 3,
        repository_id: 'repo',
        repository_version: 2,
        source: { kind: 'default_reference' },
        mapping: 'title_description_acceptance_v1',
      },
    },
    task_digest: 'a'.repeat(64),
    task_size: 24,
    acceptance_digest: 'b'.repeat(64),
    acceptance_size: 20,
    title_digest: 'c'.repeat(64),
    description_digest: 'd'.repeat(64),
    association:
      state === 'pending'
        ? null
        : {
            project_id: projectId,
            issue_id: issueFixture(projectId).id,
            request_id: id,
            requirements_version: 1,
            work_id: 'retained-work',
            run_id: 'retained-run',
          },
    run_url: state === 'pending' ? null : '/api/v1/factory-runs/retained-run',
  };
}
