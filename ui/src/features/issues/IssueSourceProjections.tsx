import type { UseQueryResult } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { WorkflowResource } from '../../api/issueWorkflows';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { IssueSourceProjection, IssueSourceProjectionPage } from './sourceApi';

/** Reporter delivery never supplies a task status or an execution acceptance verdict. */
export function IssueSourceProjections({
  query,
  workflow,
  after,
  onPage,
  onResolve,
}: {
  query: UseQueryResult<IssueSourceProjectionPage, Error>;
  workflow: WorkflowResource | undefined;
  after: string | null;
  onPage: (cursor: string | null) => void;
  onResolve: (row: IssueSourceProjection) => void;
}) {
  const { t, locale } = usePresentation();
  return (
    <section aria-label={t('issueSync.outbound')}>
      <h3>{t('issueSync.outbound')}</h3>
      <p>{t('issueSync.independent')}</p>
      {query.isPending ? (
        <QueryLoadingNotice label={t('issueSync.outbound')} />
      ) : !query.data ? (
        <QueryFailureNotice
          title={t('issueSync.outbound')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issueSync.outbound')}
            fetching={query.isFetching}
            error={query.error}
            onRetry={query.refetch}
          />
          <p>{t('issueSync.live')}</p>
          {!query.data.items.length ? (
            <p>{t('issueSync.empty')}</p>
          ) : (
            <ol>
              {query.data.items.map((row) => (
                <li key={row.operation_id}>
                  <p>{t(`issueSync.${row.failure ?? row.state}`)}</p>
                  <p>
                    {t('issues.revision', { version: row.issue_version })} ·{' '}
                    {workflow?.id === row.workflow.id && workflow.version === row.workflow.version
                      ? (workflow.definition.statuses.find((s) => s.id === row.status_id)?.name ??
                        row.status_id)
                      : row.status_id}
                  </p>
                  <p>
                    {t('issueSync.desired')}:{' '}
                    {row.effects
                      ? `+${row.effects.add.join(', ')} −${row.effects.remove.join(', ')} →${row.effects.state ?? t('factory.preserveState')}`
                      : t('issueSource.condition.unmapped')}
                  </p>
                  {row.pipeline ? (
                    <Link to={`/factory/${encodeURIComponent(row.pipeline.run_id)}`}>
                      {t('issueSync.execution')}
                    </Link>
                  ) : (
                    <p>{t('issueSync.metadata')}</p>
                  )}
                  <p>
                    {t('issueSync.acceptedAt', {
                      time: new Date(row.accepted_at).toLocaleString(locale),
                    })}
                  </p>
                  <details>
                    <summary>{t('issueSync.provenance')}</summary>
                    <p>
                      {row.operation_id} · {row.record_id}
                    </p>
                    <p>
                      {row.mapping.identity} · {row.mapping.version} · {row.mapping.digest}
                    </p>
                    <p>{row.native_correlation}</p>
                    <p>
                      {t('issueSync.attempt', { attempt: row.attempt })} ·{' '}
                      {new Date(row.recorded_at).toLocaleString(locale)}
                    </p>
                    {row.pipeline ? (
                      <p>
                        {row.pipeline.work_id} · {row.pipeline.flow_run_id} ·{' '}
                        {row.pipeline.cycle_id} · {row.pipeline.node_attempt_id} ·{' '}
                        {row.pipeline.operation_id} · {row.pipeline.outcome}
                      </p>
                    ) : null}
                    {row.resolves ? (
                      <p>
                        {t('issueSync.resolves')}: {row.resolves}
                      </p>
                    ) : null}
                  </details>
                  {row.state === 'failed' ? (
                    <button
                      type="button"
                      disabled={query.isFetching || !!query.error}
                      onClick={() => onResolve(row)}
                    >
                      {t('issueSync.review')}
                    </button>
                  ) : null}
                </li>
              ))}
            </ol>
          )}
          {after ? (
            <button type="button" onClick={() => onPage(null)}>
              {t('issues.firstPage')}
            </button>
          ) : null}
          {query.data.next_cursor ? (
            <button type="button" onClick={() => onPage(query.data!.next_cursor)}>
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
      <button type="button" disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issueSync.refresh')}
      </button>
    </section>
  );
}
