import { ManagementApiError } from '../api/client';
import { usePresentation } from '../app/presentation/PresentationProvider';

/** Closed server reasons shared by authoring and execution review. */
export const ISSUE_SNAPSHOT_PROBLEM_LABELS = {
  issue_snapshot_missing_value: 'issues.snapshotProblem.missing',
  issue_snapshot_incompatible_value: 'issues.snapshotProblem.incompatible',
  issue_snapshot_semantic_binding_changed: 'issues.snapshotProblem.binding',
  issue_snapshot_inapplicable: 'issues.snapshotProblem.stale',
  issue_snapshot_evidence_unavailable: 'issues.snapshotProblem.evidence',
  issue_snapshot_policy_ineligible: 'issues.snapshotProblem.policy',
  issue_snapshot_encoding_limit_exceeded: 'issues.snapshotProblem.encoding',
  issue_snapshot_material_limit_exceeded: 'issues.snapshotProblem.materialLimit',
  issue_snapshot_material_unavailable: 'issues.snapshotProblem.material',
} as const;
export function IssueSnapshotProblems({ error }: { error: unknown }) {
  const { t } = usePresentation();
  return error instanceof ManagementApiError && error.validationIssues.length ? (
    <ul role="alert">
      {error.validationIssues.map((problem) => {
        const label = Object.entries(ISSUE_SNAPSHOT_PROBLEM_LABELS).find(
          ([code]) => code === problem.code,
        )?.[1];
        return (
          <li key={`${problem.field}:${problem.code}`}>
            <span>{problem.field}</span> · {label ? t(label) : problem.code}
          </li>
        );
      })}
    </ul>
  ) : null;
}
