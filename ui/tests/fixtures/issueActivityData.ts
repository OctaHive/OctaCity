import type {
  IssueComment,
  IssueTimelinePage,
  IssueResultPage,
} from '../../src/features/issues/activityApi';
import { issueFixture } from './issues';
export const comment: IssueComment = {
  id: '11111111-1111-4111-8111-111111111111',
  project_id: 'delivery',
  issue_id: issueFixture().id,
  sequence: 1,
  parent_id: null,
  actor: { kind: 'unauthenticated_management', identity: null },
  created_at: 1700000000000,
  body_digest: 'a'.repeat(64),
  body_bytes: 20,
};
export function timelineFixture(
  items: IssueTimelinePage['items'] = [{ kind: 'discussion', comment }],
): IssueTimelinePage {
  return {
    items,
    next_cursor: null,
    observation: 'issue-timeline-watermark-v1',
    boundary: { issue_version: 1, executions: 1, discussion: 1, source: 'b'.repeat(64) },
    current_version: 1,
    source_stale: false,
    source_unavailable: false,
    observed_at: 1700000000000,
  };
}
export function resultsFixture(items: IssueResultPage['items'] = []): IssueResultPage {
  return {
    association: {
      project_id: 'delivery',
      issue_id: issueFixture().id,
      work_id: 'retained-work',
      run_id: 'retained-run',
      request_id: 'request-one',
      requirements_version: 1,
    },
    observation: 'issue-result-run-version-v1',
    boundary: timelineFixture().boundary,
    current_version: 1,
    source_stale: false,
    source_unavailable: false,
    run_version: 1,
    coverage: 'partial',
    items,
    next_cursor: null,
  };
}
