import type { components } from '../../.generated/api/schema';
import { executionFixture } from './issueExecutions';
import type { IssueResultPanelsApi } from '../../src/features/issues/resultPanelsApi';
export const record = 'ab'.repeat(32);
export const execution = executionFixture('admitted', 'old-request');
export const subject = {
  exact: { project_id: 'delivery', repository_id: 'repo', base_revision: 'original-base' },
  candidate_revision: 'historical-head',
  changeset_digest: 'cd'.repeat(32),
};
export function checksPage(): components['schemas']['IssueChecksPage'] {
  return {
    association: execution.association!,
    subject,
    record,
    run_version: 3,
    coverage: 'partial',
    next_cursor: null,
    items: [
      {
        id: 'a'.repeat(64),
        name: 'Required unit tests <script>',
        required: true,
        state: 'missing',
        observed_at: null,
        producer: null,
        record: null,
      },
      {
        id: 'b'.repeat(64),
        name: 'Observed lint',
        required: true,
        state: 'pass',
        observed_at: 1700000000000,
        producer: null,
        record,
      },
    ],
  };
}
export function artifactsPage(): components['schemas']['IssueArtifactsPage'] {
  const artifact = (
    id: string,
    group: components['schemas']['IssueArtifactProjection']['group'],
    availability: 'available' | 'expired',
    name: string,
    origin: components['schemas']['IssueArtifactMetadata']['origin'],
  ): components['schemas']['IssueArtifactProjection'] => ({
    artifact: { artifact_id: id, content_digest: '12'.repeat(32), encoded_size: 120 },
    group,
    availability,
    metadata: {
      name,
      media_type: 'text/plain',
      origin,
      availability,
      published_at: 1700000000000,
      expires_at: 1700000900000,
    },
    producer: null,
    record: null,
    subject: null,
  });
  return {
    association: execution.association!,
    coverage: 'partial',
    run_version: 3,
    next_cursor: 'artifact-tail',
    items: [
      artifact('input-one', 'original_input', 'available', 'Original task', 'project_input'),
      artifact('report-one', 'report', 'expired', 'Expired report', 'build_output'),
      {
        ...artifact('hidden-bytes', 'output', 'available', 'unused', 'build_output'),
        metadata: null,
        availability: 'unavailable',
      },
    ],
  };
}
export function fakeResultPanels(vi: (typeof import('vitest'))['vi']) {
  return {
    checks: vi.fn<IssueResultPanelsApi['checks']>().mockResolvedValue(checksPage()),
    artifacts: vi.fn<IssueResultPanelsApi['artifacts']>().mockResolvedValue(artifactsPage()),
    externalChecks: vi.fn<IssueResultPanelsApi['externalChecks']>(),
    content: vi.fn<IssueResultPanelsApi['content']>(),
  };
}
