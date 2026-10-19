import { useState } from 'react';
import { Link } from 'react-router-dom';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { formatEnumLabel } from '../../shared/display';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import type { Issue, IssuesApi } from './api';
import styles from './Issues.module.css';
export function IssueHistory({
  api,
  workflows,
  issue,
  preview = false,
}: {
  api: IssuesApi;
  workflows: IssueWorkflowsApi;
  issue: Issue;
  preview?: boolean;
}) {
  const { t } = usePresentation();
  const [after, setAfter] = useState<number | null>(null);
  const cursor = preview
    ? issue.version > bounds.defaultPageSize
      ? issue.version - bounds.defaultPageSize
      : null
    : after;
  const label = t(preview ? 'issues.activityPreview' : 'issues.history');
  const history = useQuery({
    queryKey: queryKeys.issueHistory(issue.project_id, issue.id, cursor),
    gcTime: 0,
    queryFn: ({ signal }) => api.history(issue.project_id, issue.id, cursor, signal),
  });
  return (
    <section className={styles.panel} aria-label={label}>
      <h2>{label}</h2>
      {history.isPending ? (
        <QueryLoadingNotice label={label} />
      ) : history.data === undefined ? (
        <QueryFailureNotice title={label} error={history.error} onRetry={history.refetch} />
      ) : (
        <>
          <QueryBackgroundNotice
            label={label}
            fetching={history.isFetching}
            error={history.error}
            onRetry={history.refetch}
          />
          {history.data.items.length ? (
            <ol className={styles.list}>
              {(preview ? history.data.items.slice(-5) : history.data.items).map((entry) => (
                <HistoryRow
                  key={entry.version}
                  entry={entry}
                  workflows={workflows}
                  projectId={issue.project_id}
                />
              ))}
            </ol>
          ) : (
            <QueryEmptyNotice>
              {t(preview && cursor !== null ? 'issues.noRecentActivity' : 'issues.noActivity')}
            </QueryEmptyNotice>
          )}
          {preview ? (
            <>
              <p>{t('issues.previewCoverage', { count: bounds.defaultPageSize })}</p>
              <Link to="?section=activity">{t('issues.fullHistory')}</Link>
            </>
          ) : null}
          {!preview && after ? (
            <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button>
          ) : null}
          {!preview && history.data.next_cursor ? (
            <button onClick={() => setAfter(history.data!.next_cursor)}>
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
    </section>
  );
}
function HistoryRow({
  entry,
  workflows,
  projectId,
}: {
  entry: components['schemas']['IssueStatusHistoryEntry'];
  workflows: IssueWorkflowsApi;
  projectId: string;
}) {
  const { t, locale } = usePresentation();
  const workflow = useQuery({
    queryKey: queryKeys.issueWorkflow(projectId, entry.workflow_id, entry.workflow_version),
    queryFn: ({ signal }) =>
      workflows.get(projectId, entry.workflow_id, entry.workflow_version, signal),
  });
  const previous = entry.previous_workflow;
  const old = useQuery({
    queryKey: queryKeys.issueWorkflow(projectId, previous?.id ?? null, previous?.version),
    enabled: previous !== null,
    queryFn: ({ signal }) => workflows.get(projectId, previous!.id, previous!.version, signal),
  });
  const from =
    (previous ? old.data : workflow.data)?.definition.statuses.find((s) => s.id === entry.from)
      ?.name ?? t('issues.unknownStatus');
  const to =
    workflow.data?.definition.statuses.find((s) => s.id === entry.to)?.name ??
    t('issues.unknownStatus');
  return (
    <li>
      <strong>
        {entry.from === null ? t('issues.created') : from} → {to}
      </strong>
      <p>
        {t('issues.revision', { version: entry.version })} · {t(`issues.origin.${entry.origin}`)}
      </p>
      <p>
        {formatEnumLabel(entry.actor.kind, t)}
        {entry.actor.identity ? ` · ${entry.actor.identity}` : ''}
      </p>
      <time dateTime={new Date(entry.occurred_at).toISOString()}>
        {new Date(entry.occurred_at).toLocaleString(locale)}
      </time>
      {entry.reason ? <p className={styles.prose}>{entry.reason}</p> : null}
      {previous ? <p>{t('issues.adopted')}</p> : null}
      {workflow.error || old.error ? <p>{t('issues.historyNamesUnavailable')}</p> : null}
    </li>
  );
}
