import { useQuery } from '@tanstack/react-query';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { FactoryDefinitions } from './FactoryDefinitions';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import type { FactoryApi } from './api';
import { factorySearch, readFactoryFilters, readFactoryProjectId } from './filters';
import styles from './Factory.module.css';

export function FactoryLandingView({ api }: { api: FactoryApi }) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const filters = readFactoryFilters(params);
  const projectId = readFactoryProjectId(params);
  const query = useCursorPage(queryKeys.factoryRuns(projectId, filters), (cursor, signal) =>
    api.listRuns(projectId, filters, cursor, signal),
  );
  return (
    <section className={styles.workspace}>
      <h1>{t('section.factory')}</h1>
      <p>{t('factory.introduction')}</p>
      {projectId ? <FactoryDefinitions key={projectId} api={api} projectId={projectId} /> : null}
      <PagedExplorerBranch
        query={query}
        empty={t('factory.noRuns')}
        labels={{
          failure: t('section.factory'),
          loading: t('section.factory'),
          stale: t('section.factory'),
          loadMore: t('factory.moreRuns'),
          loadingMore: t('section.factory'),
        }}
      >
        {(items) => (
          <ul>
            {items.map((run) => (
              <li key={run.id}>
                <Link to={`/factory/${encodeURIComponent(run.id)}${factorySearch(filters)}`}>
                  {run.external_identity}
                </Link>
                <span> · {run.state}</span>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
    </section>
  );
}

export function FactoryRunView({ api }: { api: FactoryApi }) {
  const { factoryRunId } = useParams();
  const [params] = useSearchParams();
  const { t, preferences, toggleFavorite } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.factoryRun(factoryRunId ?? null),
    queryFn: ({ signal }) => {
      if (factoryRunId === undefined) throw new Error('Factory Run route is missing');
      return api.getRun(factoryRunId, signal);
    },
  });
  if (query.isPending) return <QueryLoadingNotice label={t('factory.run')} />;
  if (query.data === undefined)
    return (
      <QueryFailureNotice error={query.error} title={t('factory.run')} onRetry={query.refetch} />
    );
  const ownerParams = new URLSearchParams(factorySearch(readFactoryFilters(params)));
  ownerParams.set('project_id', query.data.project_id);
  ownerParams.set('configuration_id', query.data.configuration_id);
  ownerParams.set('configuration_version', String(query.data.configuration_version));
  const favorite = preferences.favorites.some(
    (identity) => identity.kind === 'factory_run' && identity.id === query.data.id,
  );
  return (
    <section className={styles.workspace}>
      <h1>{t('factory.run')}</h1>
      <button
        type="button"
        aria-pressed={favorite}
        onClick={() => toggleFavorite({ kind: 'factory_run', id: query.data.id })}
      >
        {t(favorite ? 'factory.removeFavorite' : 'factory.addFavorite')}
      </button>
      <Link to={`/factory?${ownerParams.toString()}`}>{t('factory.openProject')}</Link>
      <h2>{query.data.external_identity}</h2>
      <p>{query.data.state}</p>
    </section>
  );
}
