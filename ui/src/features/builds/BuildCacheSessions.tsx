import { useQuery } from '@tanstack/react-query';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatBytes, formatEnumLabel, formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { BuildResultDiagnosticsApi, CacheSessionPage } from './api';
import styles from './Builds.module.css';

export function BuildCacheSessions({
  api,
  buildId,
}: {
  api: BuildResultDiagnosticsApi;
  buildId: string;
}) {
  const { locale, t } = usePresentation();
  const sessions = useQuery({
    queryFn: ({ signal }) => api.listBuildCacheSessions(buildId, signal),
    queryKey: queryKeys.buildCacheSessions(buildId),
  });
  return (
    <section aria-label={t('cache.label')} className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>{t('cache.eyebrow')}</p>
          <h2>{t('cache.label')}</h2>
        </div>
      </div>
      {sessions.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label={t('cache.label')} />
      ) : sessions.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={sessions.error}
          onRetry={sessions.refetch}
          title={t('cache.loadFailure')}
        />
      ) : sessions.data.items.length === 0 ? (
        <QueryEmptyNotice className={styles.diagnosticEmpty}>{t('cache.empty')}</QueryEmptyNotice>
      ) : (
        <CacheSessionTable items={sessions.data.items} locale={locale} />
      )}
      {sessions.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={sessions.error}
          fetching={sessions.isFetching}
          label={t('cache.data')}
          onRetry={sessions.refetch}
        />
      )}
    </section>
  );
}

function CacheSessionTable({
  items,
  locale,
}: {
  items: CacheSessionPage['items'];
  locale: string;
}) {
  const { t } = usePresentation();
  return (
    <div className={styles.tableScroller}>
      <table aria-label={t('cache.label')} className={styles.diagnosticTable}>
        <thead>
          <tr>
            <th scope="col">{t('cache.namespace')}</th>
            <th scope="col">{t('cache.state')}</th>
            <th scope="col">{t('cache.access')}</th>
            <th scope="col">{t('cache.quota')}</th>
            <th scope="col">{t('cache.binding')}</th>
            <th scope="col">{t('cache.lifecycle')}</th>
          </tr>
        </thead>
        <tbody>
          {items.map((session) => {
            const expires = formatTimestamp(session.expires_at_unix_ms, locale);
            const retained = formatTimestamp(session.retention_until_unix_ms, locale);
            return (
              <tr key={session.id}>
                <th scope="row">
                  <span className={styles.outputName}>{session.namespace}</span>
                  <span className={styles.secondaryMetadata}>{session.id}</span>
                </th>
                <td>
                  <span
                    className={`${styles.status} ${
                      session.state === 'active' ? styles.state_success : styles.state_danger
                    }`}
                  >
                    {formatEnumLabel(session.state, t)}
                  </span>
                </td>
                <td>{cacheAccessLabel(session, t)}</td>
                <td>{formatBytes(session.quota_bytes, locale)}</td>
                <td>
                  <span className={styles.outputName}>
                    {t('cache.agentBinding', { id: session.agent_id })}
                  </span>
                  <span className={styles.secondaryMetadata}>
                    {t('cache.jobBinding', { id: session.job_id })}
                  </span>
                </td>
                <td>
                  <span className={styles.outputName}>
                    {t('cache.expires')}{' '}
                    <time dateTime={expires.machine ?? undefined}>{expires.display}</time>
                  </span>
                  <span className={styles.secondaryMetadata}>
                    {t('cache.retainedThrough')}{' '}
                    <time dateTime={retained.machine ?? undefined}>{retained.display}</time>
                  </span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function cacheAccessLabel(
  session: CacheSessionPage['items'][number],
  t: ReturnType<typeof usePresentation>['t'],
): string {
  if (session.read && session.write) return t('cache.readWrite');
  if (session.read) return t('cache.readOnly');
  if (session.write) return t('cache.writeOnly');
  return t('cache.noAccess');
}
