import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { AlertTriangle, Search } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type {
  BuildLogSearchApi,
  BuildLogSearchFilters,
  BuildLogSearchMode,
  BuildLogSearchPage,
  BuildLogStream,
  JobResource,
} from './api';
import styles from './Builds.module.css';

const MAX_LOG_QUERY_BYTES = 1_024;
const LOG_QUERY_PARAMETER = 'log_query';
const LOG_MODE_PARAMETER = 'log_mode';
const LOG_SCOPE_PARAMETER = 'log_scope';
const LOG_STREAM_PARAMETER = 'log_stream';

type JobOption = Pick<JobResource, 'id' | 'pipeline_node_id'>;

interface BuildLogSearchProps {
  api: BuildLogSearchApi;
  attemptId: string;
  buildId: string;
  jobs: JobOption[];
  projectId: string;
}

interface LogSearchDraft {
  mode: BuildLogSearchMode;
  query: string;
  scope: string;
  stream: BuildLogStream | '';
}

/** Searches only server-redacted logs belonging to the Build shown by this route. */
export function BuildLogSearch({ api, attemptId, buildId, jobs, projectId }: BuildLogSearchProps) {
  const [searchParameters, setSearchParameters] = useSearchParams();
  const urlState = searchParameters.toString();
  const filters = readFilters(searchParameters, buildId, attemptId, jobs);
  const draft = readDraft(searchParameters, jobs);
  const [validationError, setValidationError] = useState<string | null>(null);
  const urlValidationError = filters.query === '' ? null : validateQuery(filters.query);
  const scopeValidationError = validateScope(searchParameters, jobs);
  const visibleValidationError = validationError ?? urlValidationError ?? scopeValidationError;

  const search = useInfiniteQuery<
    BuildLogSearchPage,
    Error,
    InfiniteData<BuildLogSearchPage>,
    ReturnType<typeof queryKeys.buildLogs>,
    string | null
  >({
    enabled: filters.query !== '' && urlValidationError === null && scopeValidationError === null,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => api.searchBuildLogs(projectId, filters, pageParam, signal),
    queryKey: queryKeys.buildLogs(
      projectId,
      buildId,
      filters.attemptId,
      filters.jobId,
      filters.mode,
      filters.query,
      filters.stream,
    ),
  });
  const pages = search.data?.pages ?? [];
  const hits = pages.flatMap((page) => page.items);
  const freshness = pages.at(-1)?.freshness;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    const query = String(fields.get(LOG_QUERY_PARAMETER) ?? '');
    const mode = fields.get(LOG_MODE_PARAMETER) === 'full_text' ? 'full_text' : 'literal';
    const scope = String(fields.get(LOG_SCOPE_PARAMETER) ?? 'build');
    const streamValue = fields.get(LOG_STREAM_PARAMETER);
    const stream = streamValue === 'stdout' || streamValue === 'stderr' ? streamValue : '';
    const queryError = validateQuery(query);
    if (queryError !== null) {
      setValidationError(queryError);
      return;
    }

    setValidationError(null);
    const next = withoutLogParameters(searchParameters);
    next.set(LOG_QUERY_PARAMETER, query);
    next.set(LOG_MODE_PARAMETER, mode);
    if (scope !== 'build') next.set(LOG_SCOPE_PARAMETER, scope);
    if (stream !== '') next.set(LOG_STREAM_PARAMETER, stream);
    if (next.toString() === searchParameters.toString()) {
      void search.refetch();
    } else {
      setSearchParameters(next);
    }
  }

  function clear() {
    setValidationError(null);
    setSearchParameters(withoutLogParameters(searchParameters));
  }

  return (
    <section aria-label="Redacted log search" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Server-redacted evidence</p>
          <h2>Log search</h2>
        </div>
        {filters.query === '' ? null : (
          <button className={styles.textButton} onClick={clear} type="button">
            Clear search
          </button>
        )}
      </div>
      <form className={styles.logSearchForm} key={urlState} onSubmit={submit}>
        <label className={styles.logQueryField}>
          <span>Log query</span>
          <input
            aria-invalid={
              validationError === null && urlValidationError === null ? undefined : true
            }
            defaultValue={draft.query}
            name={LOG_QUERY_PARAMETER}
            required
            type="search"
          />
        </label>
        <label>
          <span>Search mode</span>
          <select defaultValue={draft.mode} name={LOG_MODE_PARAMETER}>
            <option value="literal">Literal</option>
            <option value="full_text">Full text</option>
          </select>
        </label>
        <label>
          <span>Search scope</span>
          <select defaultValue={draft.scope} name={LOG_SCOPE_PARAMETER}>
            <option value="build">Entire Build</option>
            <option value="attempt">Current Attempt</option>
            {jobs.map((job) => (
              <option key={job.id} value={`job:${job.id}`}>
                Job: {job.pipeline_node_id}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Log stream</span>
          <select defaultValue={draft.stream} name={LOG_STREAM_PARAMETER}>
            <option value="">All streams</option>
            <option value="stdout">stdout</option>
            <option value="stderr">stderr</option>
          </select>
        </label>
        <button className={styles.secondaryButton} type="submit">
          <Search aria-hidden="true" size={15} />
          Search logs
        </button>
      </form>
      {visibleValidationError === null ? null : (
        <p className={styles.logValidation} role="alert">
          {visibleValidationError}
        </p>
      )}
      {filters.query === '' ? (
        <p className={styles.logInitial}>Search bounded, server-redacted Build output.</p>
      ) : urlValidationError !== null || scopeValidationError !== null ? null : search.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label="redacted logs" />
      ) : search.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={search.error}
          onRetry={search.refetch}
          title="Redacted logs could not be loaded."
        />
      ) : (
        <>
          <QueryBackgroundNotice
            className={styles.staleNotice}
            error={search.error}
            fetching={search.isFetching && !search.isFetchingNextPage}
            label="redacted log search data"
            onRetry={search.refetch}
          />
          {freshness?.caught_up === false ? <FreshnessWarning freshness={freshness} /> : null}
          {hits.length === 0 ? (
            <QueryEmptyNotice className={styles.diagnosticEmpty}>
              {freshness?.caught_up === false
                ? 'The search projection is still catching up; no matches are authoritative yet.'
                : 'No redacted log matches the selected filters.'}
            </QueryEmptyNotice>
          ) : (
            <LogHits hits={hits} />
          )}
          {search.hasNextPage ? (
            <button
              className={styles.secondaryButton}
              disabled={search.isFetchingNextPage}
              onClick={() => void search.fetchNextPage()}
              type="button"
            >
              {search.isFetchingNextPage ? 'Loading more matches' : 'Load more log matches'}
            </button>
          ) : null}
        </>
      )}
    </section>
  );
}

function FreshnessWarning({ freshness }: { freshness: BuildLogSearchPage['freshness'] }) {
  return (
    <div className={styles.logFreshness} role="status">
      <AlertTriangle aria-hidden="true" size={16} />
      Results may be incomplete. Indexed through {watermark(freshness.indexed_through)}; committed
      through {watermark(freshness.committed_through)}.
    </div>
  );
}

function LogHits({ hits }: { hits: BuildLogSearchPage['items'] }) {
  return (
    <ol className={styles.logHits}>
      {hits.map((hit) => {
        const occurred = formatTimestamp(hit.occurred_at_unix_ms);
        return (
          <li className={styles.logHit} key={hit.chunk_id}>
            <div className={styles.logHitMetadata}>
              <strong>{hit.stream}</strong>
              <span>
                sequences {hit.first_sequence}–{hit.last_sequence}
              </span>
              <span>{hit.job_id}</span>
              <time dateTime={occurred.machine ?? undefined}>{occurred.display}</time>
            </div>
            <pre>{hit.snippet}</pre>
          </li>
        );
      })}
    </ol>
  );
}

function readFilters(
  parameters: URLSearchParams,
  buildId: string,
  attemptId: string,
  jobs: JobOption[],
): BuildLogSearchFilters {
  const draft = readDraft(parameters, jobs);
  const jobId = jobIdFromScope(draft.scope, jobs);
  return {
    attemptId: draft.scope === 'attempt' || jobId !== null ? attemptId : null,
    buildId,
    jobId,
    mode: draft.mode,
    query: parameters.get(LOG_QUERY_PARAMETER) ?? '',
    stream: draft.stream === '' ? null : draft.stream,
  };
}

function readDraft(parameters: URLSearchParams, jobs: JobOption[]): LogSearchDraft {
  const rawMode = parameters.get(LOG_MODE_PARAMETER);
  const rawStream = parameters.get(LOG_STREAM_PARAMETER);
  const rawScope = parameters.get(LOG_SCOPE_PARAMETER) ?? 'build';
  return {
    mode: rawMode === 'full_text' ? 'full_text' : 'literal',
    query: parameters.get(LOG_QUERY_PARAMETER) ?? '',
    scope:
      rawScope === 'attempt' || rawScope === 'build' || jobIdFromScope(rawScope, jobs) !== null
        ? rawScope
        : 'build',
    stream: rawStream === 'stdout' || rawStream === 'stderr' ? rawStream : '',
  };
}

function jobIdFromScope(scope: string, jobs: JobOption[]): string | null {
  if (!scope.startsWith('job:')) return null;
  const jobId = scope.slice('job:'.length);
  return jobs.some((job) => job.id === jobId) ? jobId : null;
}

function validateScope(parameters: URLSearchParams, jobs: JobOption[]): string | null {
  const scope = parameters.get(LOG_SCOPE_PARAMETER);
  if (scope === null || scope === 'build' || scope === 'attempt') return null;
  return jobIdFromScope(scope, jobs) === null
    ? 'The Job selected by this log-search URL is not available in the current Attempt.'
    : null;
}

function withoutLogParameters(parameters: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(parameters);
  for (const name of [
    LOG_QUERY_PARAMETER,
    LOG_MODE_PARAMETER,
    LOG_SCOPE_PARAMETER,
    LOG_STREAM_PARAMETER,
  ]) {
    next.delete(name);
  }
  return next;
}

function watermark(value: number | null): string {
  return value === null ? 'none' : String(value);
}

function validateQuery(query: string): string | null {
  if (query.length === 0) return 'Enter a log query.';
  return new TextEncoder().encode(query).length > MAX_LOG_QUERY_BYTES
    ? `Log query must be at most ${MAX_LOG_QUERY_BYTES} UTF-8 bytes.`
    : null;
}
