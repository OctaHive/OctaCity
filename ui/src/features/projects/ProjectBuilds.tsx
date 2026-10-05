import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { ArrowUpRight, RefreshCw } from 'lucide-react';
import { useEffect } from 'react';
import { Link, useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { buildPath } from '../../app/routes';
import type {
  BuildConfigurationSummaryPage,
  BuildFilters,
  BuildState,
  BuildSummary,
  BuildSummaryPage,
  ProjectBuildsApi,
} from './api';
import { formatEnumLabel, formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import buildStyles from './ProjectBuilds.module.css';
import styles from './Projects.module.css';

const UUID_PATTERN = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const NIL_UUID = '00000000-0000-0000-0000-000000000000';

interface ProjectBuildsProps {
  api: ProjectBuildsApi;
  projectId: string;
}

export function ProjectBuilds({ api, projectId }: ProjectBuildsProps) {
  const { t } = usePresentation();
  const [searchParameters, setSearchParameters] = useSearchParams();
  const filters = readBuildFilters(searchParameters);
  const canonicalSearch = canonicalizeBuildFilters(searchParameters, filters).toString();
  useEffect(() => {
    if (canonicalSearch !== searchParameters.toString()) {
      setSearchParameters(canonicalSearch, { replace: true });
    }
  }, [canonicalSearch, searchParameters, setSearchParameters]);
  const configurations = useInfiniteQuery<
    BuildConfigurationSummaryPage,
    Error,
    InfiniteData<BuildConfigurationSummaryPage>,
    ReturnType<typeof queryKeys.projectBuildConfigurations>,
    string | null
  >({
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => api.listBuildConfigurations(projectId, pageParam, signal),
    queryKey: queryKeys.projectBuildConfigurations(projectId),
  });
  const builds = useInfiniteQuery<
    BuildSummaryPage,
    Error,
    InfiniteData<BuildSummaryPage>,
    ReturnType<typeof queryKeys.projectBuilds>,
    string | null
  >({
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => api.listBuilds(projectId, filters, pageParam, signal),
    queryKey: queryKeys.projectBuilds(projectId, filters.configurationId, filters.state),
  });

  const configurationItems = configurations.data?.pages.flatMap((page) => page.items) ?? [];
  const buildItems = builds.data?.pages.flatMap((page) => page.items) ?? [];
  const selectedConfigurationIsKnown = configurationItems.some(
    (configuration) => configuration.id === filters.configurationId,
  );

  function updateFilter(name: 'configuration_id' | 'state', value: string) {
    const next = new URLSearchParams(canonicalSearch);
    if (value === '') {
      next.delete(name);
    } else {
      next.set(name, value);
    }
    setSearchParameters(next);
  }

  return (
    <section aria-labelledby="recent-builds-heading" className={buildStyles.section}>
      <div className={`${styles.sectionHeading} ${buildStyles.sectionHeading}`}>
        <div>
          <p className={styles.eyebrow}>{t('builds.executionHistory')}</p>
          <h2 id="recent-builds-heading">{t('builds.recent')}</h2>
        </div>
        {builds.data === undefined ? null : (
          <span>{t('builds.loadedNewest', { count: buildItems.length })}</span>
        )}
      </div>
      <form className={buildStyles.filters} onSubmit={(event) => event.preventDefault()}>
        <label>
          <span>{t('builds.configuration')}</span>
          <select
            onChange={(event) => updateFilter('configuration_id', event.currentTarget.value)}
            value={filters.configurationId ?? ''}
          >
            <option value="">{t('builds.allConfigurations')}</option>
            {filters.configurationId === null || selectedConfigurationIsKnown ? null : (
              <option value={filters.configurationId}>{filters.configurationId}</option>
            )}
            {configurationItems.map((configuration) => (
              <option key={configuration.id} value={configuration.id}>
                {configuration.name} · v{configuration.version}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{t('builds.state')}</span>
          <select
            onChange={(event) => updateFilter('state', event.currentTarget.value)}
            value={filters.state ?? ''}
          >
            <option value="">{t('builds.allStates')}</option>
            {(Object.keys(BUILD_STATE_PRESENTATIONS) as BuildState[]).map((state) => (
              <option key={state} value={state}>
                {formatEnumLabel(state, t)}
              </option>
            ))}
          </select>
        </label>
        <button
          className={styles.textButton}
          disabled={builds.isFetching}
          onClick={() => void builds.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={15} />
          {builds.isFetching && !builds.isFetchingNextPage
            ? t('common.refreshing')
            : t('common.refresh')}
        </button>
      </form>
      {configurations.hasNextPage ? (
        <button
          className={buildStyles.filterOptionsButton}
          disabled={configurations.isFetchingNextPage}
          onClick={() => void configurations.fetchNextPage()}
          type="button"
        >
          {configurations.isFetchingNextPage
            ? t('builds.loadingFilterOptions')
            : t('builds.loadMoreFilterOptions')}
        </button>
      ) : null}
      {configurations.data === undefined && configurations.error !== null ? (
        <QueryFailureNotice
          className={styles.compactFailure}
          error={configurations.error}
          onRetry={configurations.refetch}
          title={t('builds.filterLoadFailure')}
        />
      ) : configurations.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={configurations.error}
          fetching={configurations.isFetching && !configurations.isFetchingNextPage}
          label={t('builds.filterData')}
          onRetry={
            configurations.isFetchNextPageError
              ? configurations.fetchNextPage
              : configurations.refetch
          }
        />
      )}
      {builds.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={builds.error}
          fetching={builds.isFetching && !builds.isFetchingNextPage}
          label={t('builds.data')}
          onRetry={builds.isFetchNextPageError ? builds.fetchNextPage : builds.refetch}
        />
      )}
      {builds.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label={t('builds.recentLower')} />
      ) : builds.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={builds.error}
          onRetry={builds.refetch}
          title={t('builds.recentLoadFailure')}
        />
      ) : buildItems.length === 0 ? (
        <QueryEmptyNotice className={styles.emptyState}>{t('builds.noMatches')}</QueryEmptyNotice>
      ) : (
        <BuildTable builds={buildItems} configurations={configurationItems} />
      )}
      {builds.hasNextPage ? (
        <button
          className={styles.loadMoreButton}
          disabled={builds.isFetchingNextPage}
          onClick={() => void builds.fetchNextPage()}
          type="button"
        >
          {builds.isFetchingNextPage ? t('builds.loading') : t('builds.loadMore')}
        </button>
      ) : null}
    </section>
  );
}

function BuildTable({
  builds,
  configurations,
}: {
  builds: BuildSummary[];
  configurations: BuildConfigurationSummaryPage['items'];
}) {
  const { locale, t } = usePresentation();
  return (
    <div className={buildStyles.tableFrame}>
      <table className={buildStyles.table}>
        <caption>{t('builds.caption')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('builds.build')}</th>
            <th scope="col">{t('builds.configurationShort')}</th>
            <th scope="col">{t('builds.cause')}</th>
            <th scope="col">{t('builds.attempt')}</th>
            <th scope="col">{t('builds.created')}</th>
            <th scope="col">{t('builds.stateShort')}</th>
          </tr>
        </thead>
        <tbody>
          {builds.map((build) => {
            const configuration = configurations.find((item) => item.id === build.configuration_id);
            const timestamp = formatTimestamp(build.created_at_unix_ms, locale);
            return (
              <tr key={build.id}>
                <td>
                  <Link className={buildStyles.link} to={buildPath(build.id)}>
                    <span>{build.id}</span>
                    <ArrowUpRight aria-hidden="true" size={14} />
                  </Link>
                </td>
                <td>
                  <strong>{configuration?.name ?? build.configuration_id}</strong>
                  <span>{t('builds.version', { version: build.configuration_version })}</span>
                </td>
                <td>{formatEnumLabel(build.cause.kind, t)}</td>
                <td>
                  <strong>
                    {t('builds.attemptNumber', { number: build.current_attempt_number })}
                  </strong>
                  <span>{formatEnumLabel(build.current_attempt_state, t)}</span>
                </td>
                <td>
                  {timestamp.machine === null ? (
                    timestamp.display
                  ) : (
                    <time dateTime={timestamp.machine}>{timestamp.display}</time>
                  )}
                </td>
                <td>
                  <span
                    className={`${buildStyles.state} ${BUILD_STATE_PRESENTATIONS[build.state].className}`}
                  >
                    {formatEnumLabel(build.state, t)}
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

function readBuildFilters(parameters: URLSearchParams): BuildFilters {
  const rawConfigurationId = parameters.get('configuration_id');
  const rawState = parameters.get('state');
  return {
    configurationId: isCanonicalUuid(rawConfigurationId) ? rawConfigurationId : null,
    state: isBuildState(rawState) ? rawState : null,
  };
}

function canonicalizeBuildFilters(
  parameters: URLSearchParams,
  filters: BuildFilters,
): URLSearchParams {
  const canonical = new URLSearchParams(parameters);
  canonical.delete('configuration_id');
  canonical.delete('state');
  if (filters.configurationId !== null) {
    canonical.set('configuration_id', filters.configurationId);
  }
  if (filters.state !== null) {
    canonical.set('state', filters.state);
  }
  return canonical;
}

function isCanonicalUuid(value: string | null): value is string {
  return value !== null && value !== NIL_UUID && UUID_PATTERN.test(value);
}

function isBuildState(value: string | null): value is BuildState {
  return value !== null && Object.hasOwn(BUILD_STATE_PRESENTATIONS, value);
}

const BUILD_STATE_PRESENTATIONS: Record<BuildState, { className: string | undefined }> = {
  queued: { className: buildStyles.stateActive },
  running: { className: buildStyles.stateActive },
  succeeded: { className: buildStyles.stateSucceeded },
  failed: { className: buildStyles.stateFailed },
  cancelled: { className: buildStyles.stateFailed },
};
