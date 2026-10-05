import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { ArrowUpRight, RefreshCw } from 'lucide-react';
import { useEffect } from 'react';
import { Link, useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
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
          <p className={styles.eyebrow}>Execution history</p>
          <h2 id="recent-builds-heading">Recent Builds</h2>
        </div>
        {builds.data === undefined ? null : <span>{buildItems.length} loaded · newest first</span>}
      </div>
      <form className={buildStyles.filters} onSubmit={(event) => event.preventDefault()}>
        <label>
          <span>Build Configuration</span>
          <select
            onChange={(event) => updateFilter('configuration_id', event.currentTarget.value)}
            value={filters.configurationId ?? ''}
          >
            <option value="">All configurations</option>
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
          <span>Build state</span>
          <select
            onChange={(event) => updateFilter('state', event.currentTarget.value)}
            value={filters.state ?? ''}
          >
            <option value="">All states</option>
            {Object.entries(BUILD_STATE_PRESENTATIONS).map(([state, presentation]) => (
              <option key={state} value={state}>
                {presentation.label}
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
          {builds.isFetching && !builds.isFetchingNextPage ? 'Refreshing' : 'Refresh'}
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
            ? 'Loading filter options'
            : 'Load more Build Configuration filter options'}
        </button>
      ) : null}
      {configurations.data === undefined && configurations.error !== null ? (
        <QueryFailureNotice
          className={styles.compactFailure}
          error={configurations.error}
          onRetry={configurations.refetch}
          title="Build Configuration filter options could not be loaded."
        />
      ) : configurations.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={configurations.error}
          fetching={configurations.isFetching && !configurations.isFetchingNextPage}
          label="Build Configuration filter option data"
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
          label="Build data"
          onRetry={builds.isFetchNextPageError ? builds.fetchNextPage : builds.refetch}
        />
      )}
      {builds.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label="recent Builds" />
      ) : builds.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={builds.error}
          onRetry={builds.refetch}
          title="Recent Builds could not be loaded."
        />
      ) : buildItems.length === 0 ? (
        <QueryEmptyNotice className={styles.emptyState}>
          No Builds match the selected filters.
        </QueryEmptyNotice>
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
          {builds.isFetchingNextPage ? 'Loading Builds' : 'Load more Builds'}
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
  return (
    <div className={buildStyles.tableFrame}>
      <table className={buildStyles.table}>
        <caption>Recent Builds in server-provided newest-first order</caption>
        <thead>
          <tr>
            <th scope="col">Build</th>
            <th scope="col">Configuration</th>
            <th scope="col">Cause</th>
            <th scope="col">Attempt</th>
            <th scope="col">Created</th>
            <th scope="col">State</th>
          </tr>
        </thead>
        <tbody>
          {builds.map((build) => {
            const configuration = configurations.find((item) => item.id === build.configuration_id);
            const timestamp = formatTimestamp(build.created_at_unix_ms);
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
                  <span>Version {build.configuration_version}</span>
                </td>
                <td>{formatEnumLabel(build.cause.kind)}</td>
                <td>
                  <strong>Attempt {build.current_attempt_number}</strong>
                  <span>{formatEnumLabel(build.current_attempt_state)}</span>
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
                    {BUILD_STATE_PRESENTATIONS[build.state].label}
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

const BUILD_STATE_PRESENTATIONS: Record<
  BuildState,
  { className: string | undefined; label: string }
> = {
  queued: { className: buildStyles.stateActive, label: 'Queued' },
  running: { className: buildStyles.stateActive, label: 'Running' },
  succeeded: { className: buildStyles.stateSucceeded, label: 'Succeeded' },
  failed: { className: buildStyles.stateFailed, label: 'Failed' },
  cancelled: { className: buildStyles.stateFailed, label: 'Cancelled' },
};
