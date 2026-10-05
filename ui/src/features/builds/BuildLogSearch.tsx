import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { AlertTriangle, Search } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatTimestamp } from '../../shared/display';
import { SelectMenu } from '../../shared/SelectMenu';
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
  const { locale, t } = usePresentation();
  const [searchParameters, setSearchParameters] = useSearchParams();
  const urlState = searchParameters.toString();
  const filters = readFilters(searchParameters, buildId, attemptId, jobs);
  const draft = readDraft(searchParameters, jobs);
  const [validationError, setValidationError] = useState<string | null>(null);
  const urlValidationError = filters.query === '' ? null : validateQuery(filters.query, t);
  const scopeValidationError = validateScope(searchParameters, jobs, t);
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
    const queryError = validateQuery(query, t);
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
    <section aria-label={t('logs.label')} className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>{t('logs.eyebrow')}</p>
          <h2>{t('logs.title')}</h2>
        </div>
        {filters.query === '' ? null : (
          <button className={styles.textButton} onClick={clear} type="button">
            {t('logs.clear')}
          </button>
        )}
      </div>
      <form className={styles.logSearchForm} key={urlState} onSubmit={submit}>
        <label className={styles.logQueryField}>
          <span>{t('logs.query')}</span>
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
        <div className={styles.logSelectField}>
          <span>{t('logs.searchMode')}</span>
          <SelectMenu
            ariaLabel={t('logs.searchMode')}
            defaultValue={draft.mode}
            name={LOG_MODE_PARAMETER}
            options={[
              { label: t('logs.literal'), value: 'literal' },
              { label: t('logs.fullText'), value: 'full_text' },
            ]}
          />
        </div>
        <div className={styles.logSelectField}>
          <span>{t('logs.scope')}</span>
          <SelectMenu
            ariaLabel={t('logs.scope')}
            defaultValue={draft.scope}
            name={LOG_SCOPE_PARAMETER}
            options={[
              { label: t('logs.buildScope'), value: 'build' },
              { label: t('logs.attemptScope'), value: 'attempt' },
              ...jobs.map((job) => ({
                label: t('logs.jobScope', { name: job.pipeline_node_id }),
                value: `job:${job.id}`,
              })),
            ]}
          />
        </div>
        <div className={styles.logSelectField}>
          <span>{t('logs.stream')}</span>
          <SelectMenu
            ariaLabel={t('logs.stream')}
            defaultValue={draft.stream}
            name={LOG_STREAM_PARAMETER}
            options={[
              { label: t('logs.allStreams'), value: '' },
              { label: 'stdout', value: 'stdout' },
              { label: 'stderr', value: 'stderr' },
            ]}
          />
        </div>
        <button className={styles.secondaryButton} type="submit">
          <Search aria-hidden="true" size={15} />
          {t('logs.search')}
        </button>
      </form>
      {visibleValidationError === null ? null : (
        <p className={styles.logValidation} role="alert">
          {visibleValidationError}
        </p>
      )}
      {filters.query === '' ? (
        <p className={styles.logInitial}>{t('logs.initial')}</p>
      ) : urlValidationError !== null || scopeValidationError !== null ? null : search.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label={t('logs.label')} />
      ) : search.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={search.error}
          onRetry={search.refetch}
          title={t('logs.loadFailure')}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            className={styles.staleNotice}
            error={search.error}
            fetching={search.isFetching && !search.isFetchingNextPage}
            label={t('logs.data')}
            onRetry={search.refetch}
          />
          {freshness?.caught_up === false ? <FreshnessWarning freshness={freshness} /> : null}
          {hits.length === 0 ? (
            <QueryEmptyNotice className={styles.diagnosticEmpty}>
              {freshness?.caught_up === false ? t('logs.emptyWhileCatchingUp') : t('logs.empty')}
            </QueryEmptyNotice>
          ) : (
            <LogHits hits={hits} locale={locale} />
          )}
          {search.hasNextPage ? (
            <button
              className={styles.secondaryButton}
              disabled={search.isFetchingNextPage}
              onClick={() => void search.fetchNextPage()}
              type="button"
            >
              {search.isFetchingNextPage ? t('logs.loadingMore') : t('logs.loadMore')}
            </button>
          ) : null}
        </>
      )}
    </section>
  );
}

function FreshnessWarning({ freshness }: { freshness: BuildLogSearchPage['freshness'] }) {
  const { t } = usePresentation();
  return (
    <div className={styles.logFreshness} role="status">
      <AlertTriangle aria-hidden="true" size={16} />
      {t('logs.freshness', {
        committed: watermark(freshness.committed_through, t),
        indexed: watermark(freshness.indexed_through, t),
      })}
    </div>
  );
}

function LogHits({ hits, locale }: { hits: BuildLogSearchPage['items']; locale: string }) {
  const { t } = usePresentation();
  return (
    <ol className={styles.logHits}>
      {hits.map((hit) => {
        const occurred = formatTimestamp(hit.occurred_at_unix_ms, locale);
        return (
          <li className={styles.logHit} key={hit.chunk_id}>
            <div className={styles.logHitMetadata}>
              <strong>{hit.stream}</strong>
              <span>
                {t('logs.sequenceRange', {
                  first: hit.first_sequence,
                  last: hit.last_sequence,
                })}
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

function validateScope(
  parameters: URLSearchParams,
  jobs: JobOption[],
  t: ReturnType<typeof usePresentation>['t'],
): string | null {
  const scope = parameters.get(LOG_SCOPE_PARAMETER);
  if (scope === null || scope === 'build' || scope === 'attempt') return null;
  return jobIdFromScope(scope, jobs) === null ? t('logs.scopeUnavailable') : null;
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

function watermark(value: number | null, t: ReturnType<typeof usePresentation>['t']): string {
  return value === null ? t('logs.none') : String(value);
}

function validateQuery(query: string, t: ReturnType<typeof usePresentation>['t']): string | null {
  if (query.length === 0) return t('logs.enterQuery');
  return new TextEncoder().encode(query).length > MAX_LOG_QUERY_BYTES
    ? t('logs.queryTooLong', { maximum: MAX_LOG_QUERY_BYTES })
    : null;
}
