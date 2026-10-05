import { useQuery } from '@tanstack/react-query';

import { queryKeys } from '../../app/query';
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
  const sessions = useQuery({
    queryFn: ({ signal }) => api.listBuildCacheSessions(buildId, signal),
    queryKey: queryKeys.buildCacheSessions(buildId),
  });
  return (
    <section aria-label="Cache sessions" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Secret-free authority</p>
          <h2>Cache sessions</h2>
        </div>
      </div>
      {sessions.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label="cache sessions" />
      ) : sessions.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={sessions.error}
          onRetry={sessions.refetch}
          title="Cache sessions could not be loaded."
        />
      ) : sessions.data.items.length === 0 ? (
        <QueryEmptyNotice className={styles.diagnosticEmpty}>
          No cache sessions were issued for this Build.
        </QueryEmptyNotice>
      ) : (
        <CacheSessionTable items={sessions.data.items} />
      )}
      {sessions.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={sessions.error}
          fetching={sessions.isFetching}
          label="cache sessions data"
          onRetry={sessions.refetch}
        />
      )}
    </section>
  );
}

function CacheSessionTable({ items }: { items: CacheSessionPage['items'] }) {
  return (
    <div className={styles.tableScroller}>
      <table aria-label="Build cache sessions" className={styles.diagnosticTable}>
        <thead>
          <tr>
            <th scope="col">Namespace</th>
            <th scope="col">State</th>
            <th scope="col">Access</th>
            <th scope="col">Quota</th>
            <th scope="col">Binding</th>
            <th scope="col">Lifecycle</th>
          </tr>
        </thead>
        <tbody>
          {items.map((session) => {
            const expires = formatTimestamp(session.expires_at_unix_ms);
            const retained = formatTimestamp(session.retention_until_unix_ms);
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
                    {formatEnumLabel(session.state)}
                  </span>
                </td>
                <td>{cacheAccessLabel(session)}</td>
                <td>{formatBytes(session.quota_bytes)}</td>
                <td>
                  <span className={styles.outputName}>Agent {session.agent_id}</span>
                  <span className={styles.secondaryMetadata}>Job {session.job_id}</span>
                </td>
                <td>
                  <span className={styles.outputName}>
                    Expires <time dateTime={expires.machine ?? undefined}>{expires.display}</time>
                  </span>
                  <span className={styles.secondaryMetadata}>
                    Retained through{' '}
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

function cacheAccessLabel(session: CacheSessionPage['items'][number]): string {
  if (session.read && session.write) return 'Read and write';
  if (session.read) return 'Read only';
  if (session.write) return 'Write only';
  return 'No access';
}
