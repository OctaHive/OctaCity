import { useQuery } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { QueryContent } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueExecution, IssueExecutionsApi } from './executionApi';
import { sameExecutionTarget } from './issueExecutionTarget';
import { IssueExecutionSnapshotView } from './IssueExecutionSnapshotView';

/** Exact retained request; no current snapshot is prepared to replace a historical input. */
export function IssueExecutionInput({
  api,
  execution,
  issue,
}: {
  api: IssueExecutionsApi;
  execution: IssueExecution;
  issue: Issue;
}) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueExecutionSnapshot(issue.project_id, issue.id, execution.id),
    gcTime: 0,
    queryFn: ({ signal }) => api.snapshot(issue.project_id, issue.id, execution.id, signal),
  });
  return (
    <section aria-label={t('issues.snapshotFrozen')}>
      <h3>{t('issues.snapshotFrozen')}</h3>
      {execution.intent.expected_version !== issue.version ? (
        <p>{t('issues.snapshotCurrentDifference', { version: issue.version })}</p>
      ) : null}
      <button type="button" disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issues.snapshotRefresh')}
      </button>
      <QueryContent query={query} label={t('issues.snapshotFrozen')}>
        {(data) =>
          data.availability === 'available' &&
          !query.error &&
          !query.isFetching &&
          data.digest === execution.snapshot_digest &&
          data.snapshot.project_id === issue.project_id &&
          data.snapshot.issue_id === issue.id &&
          data.snapshot.version === execution.intent.expected_version &&
          sameExecutionTarget(data.snapshot.target, execution.intent.target) ? (
            <IssueExecutionSnapshotView snapshot={data.snapshot} digest={data.digest} />
          ) : (
            <p>{t('issues.snapshotUnavailable')}</p>
          )
        }
      </QueryContent>
    </section>
  );
}
