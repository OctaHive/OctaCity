import type {
  IssueJournalPage,
  IssueJournalEvents,
  IssueResultPage,
} from '../../src/features/issues/activityApi';
import type { IssueExecution } from '../../src/features/issues/executionApi';
import { executionFixture } from './issueExecutions';
import { resultsFixture } from './issueActivityData';
export function journalFixture(
  items: IssueResultPage['items'] = [],
  execution: IssueExecution = executionFixture('admitted', 'old-request'),
): IssueJournalPage {
  const request = {
    id: execution.id,
    intent: execution.intent,
    requested_at: execution.requested_at,
    association: execution.association,
    task_digest: execution.task_digest,
    task_size: execution.task_size,
    acceptance_digest: execution.acceptance_digest,
    acceptance_size: execution.acceptance_size,
    title_digest: execution.title_digest,
    description_digest: execution.description_digest,
  };
  return {
    execution: {
      request,
      issue_version: 1,
      requirements: 'current',
      run: { availability: execution.association ? 'unavailable' : 'preparing' },
    },
    records: execution.association
      ? { ...resultsFixture(items), association: execution.association }
      : null,
  };
}
export function journalEventsFixture(
  association = executionFixture('admitted', 'old-request').association!,
): IssueJournalEvents {
  return {
    observation: 'issue-job-event-sequence-v1',
    cursor: 'tail.one',
    coverage: 'partial',
    job_stale: false,
    gaps: [],
    job: {
      association,
      execution: {
        id: 'a'.repeat(64),
        operation_id: 'b'.repeat(64),
        node_attempt_id: 'node-a',
        build_id: 'original-build',
        attempt_id: 'original-attempt',
        jobs: ['original-job'],
      },
      profile: null,
      job: {
        id: 'original-job',
        state: 'failed',
        version: 1,
        created_at: 1000,
        updated_at: 3000,
        assignment: { selected_pool_id: 'workers', assigned_agent_id: 'agent-one' },
        terminal: { state: 'failed', failure: 'execution', completed_at: 3000 },
      },
      log: {
        build_id: 'original-build',
        attempt_id: 'original-attempt',
        job_id: 'original-job',
        event_cursor: 3,
        events_url: '/api/v1/jobs/original-job/events',
      },
    },
    events: [
      {
        sequence: 1,
        occurred_at_unix_ms: 1100,
        detail_url: '/unused/1',
        facts: {
          kind: 'operation',
          runner_sequence: 1,
          run_id: 1,
          step_id: 1,
          command_id: 'command-one',
          observation: {
            operation_id: 'c'.repeat(64),
            phase: 'completed',
            observed_at_unix_ms: 1100,
            operation: { kind: 'command', exit_code: 17, failed: true },
          },
        },
      },
      {
        sequence: 2,
        occurred_at_unix_ms: 1200,
        detail_url: '/unused/2',
        facts: {
          kind: 'operation',
          runner_sequence: 2,
          run_id: 1,
          step_id: 1,
          command_id: null,
          observation: {
            operation_id: 'd'.repeat(64),
            phase: 'completed',
            observed_at_unix_ms: 1200,
            operation: { kind: 'file', path: 'src/main.rs', change: 'modified', failed: false },
          },
        },
      },
      {
        sequence: 3,
        occurred_at_unix_ms: 1300,
        detail_url: '/unused/3',
        facts: { kind: 'not_reported' },
      },
    ],
  };
}
