import { useState } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { ConnectionEditor } from './ConnectionEditor';
import { integrationsApi, safeIntegrationError, type IntegrationsApi } from './api';
import styles from './Integrations.module.css';

/** Reusable connection administration is independent of Factory intake and optional task cards. */
export function IntegrationsView({ api = integrationsApi }: { api?: IntegrationsApi | undefined }) {
  const { t } = usePresentation();
  const [creating, setCreating] = useState(false);
  const connections = useInfiniteQuery({
    queryKey: queryKeys.integrationList,
    queryFn: ({ pageParam, signal }) => api.listConnections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const items = connections.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <div className={styles.view}>
      <section className={styles.card} aria-labelledby="integrations-heading">
        <h1 id="integrations-heading">{t('section.integrations')}</h1>
        <p>{t('integrations.separateAccess')}</p>
        <div className={styles.actions}>
          <button
            type="button"
            disabled={creating || connections.isPending || !!connections.error}
            onClick={() => setCreating(true)}
          >
            {t('integrations.create')}
          </button>
          <Link to="/factory">{t('integrations.manualIntake')}</Link>
        </div>
      </section>
      {creating ? <ConnectionEditor api={api} onClose={() => setCreating(false)} /> : null}
      {connections.isPending ? <QueryLoadingNotice label={t('section.integrations')} /> : null}
      {connections.error && !connections.data ? (
        <QueryFailureNotice
          title={t('section.integrations')}
          error={safeIntegrationError(connections.error, t('integrations.safeFailure'))}
          onRetry={() => connections.refetch()}
        />
      ) : null}
      {connections.data ? (
        <QueryBackgroundNotice
          label={t('section.integrations')}
          error={
            connections.error
              ? safeIntegrationError(connections.error, t('integrations.safeFailure'))
              : null
          }
          fetching={connections.isFetching && !connections.isFetchingNextPage}
          onRetry={() =>
            connections.isFetchNextPageError ? connections.fetchNextPage() : connections.refetch()
          }
        />
      ) : null}
      {!connections.isPending && !connections.error && !items.length ? (
        <QueryEmptyNotice>{t('integrations.empty')}</QueryEmptyNotice>
      ) : null}
      <ul className={styles.list}>
        {items.map((connection) => (
          <li key={connection.id} className={styles.card}>
            <h2>
              <Link to={`/integrations/${encodeURIComponent(connection.id)}`}>
                {connection.definition.name}
              </Link>
            </h2>
            <p>
              {connection.definition.provider} · {connection.definition.endpoint}
            </p>
            <p>
              {t('integrations.metadataVersion')} · {connection.version}
            </p>
          </li>
        ))}
      </ul>
      {connections.hasNextPage ? (
        <button
          type="button"
          disabled={connections.isFetchingNextPage}
          onClick={() => void connections.fetchNextPage()}
        >
          {t('integrations.moreConnections')}
        </button>
      ) : null}
    </div>
  );
}
