import { useState } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { SourceAdmissions } from '../../shared/SourceAdmissions';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { safeIntegrationError, type IntegrationsApi, type SourceRecord } from './api';

/** Visible retained versions, scoped to one connection/Project; never an authorization decision. */
export function ConnectionDependencies({
  api,
  id,
  project,
}: {
  api: IntegrationsApi;
  id: string;
  project: string;
}) {
  const { t } = usePresentation();
  const dependencies = useInfiniteQuery({
    queryKey: queryKeys.integrationDependencies(id, project),
    queryFn: ({ pageParam, signal }) => api.listDependencies(id, project, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const items = dependencies.data?.pages.flatMap((p) => p.items) ?? [];
  const error = dependencies.error
    ? safeIntegrationError(dependencies.error, t('integrations.safeFailure'))
    : null;
  return (
    <section aria-labelledby="connection-dependencies-heading">
      <h4 id="connection-dependencies-heading">{t('integrations.dependencies')}</h4>
      <p>{t('integrations.dependenciesHelp')}</p>
      {dependencies.isPending ? (
        <QueryLoadingNotice label={t('integrations.dependencies')} />
      ) : null}
      {!dependencies.data && error ? (
        <QueryFailureNotice
          title={t('integrations.dependencies')}
          error={error}
          onRetry={() => dependencies.refetch()}
        />
      ) : null}
      {dependencies.data ? (
        <>
          <QueryBackgroundNotice
            label={t('integrations.dependencies')}
            error={error}
            fetching={dependencies.isFetching && !dependencies.isFetchingNextPage}
            onRetry={() =>
              dependencies.isFetchNextPageError
                ? dependencies.fetchNextPage()
                : dependencies.refetch()
            }
          />
          {!items.length ? (
            <QueryEmptyNotice>{t('integrations.noDependencies')}</QueryEmptyNotice>
          ) : (
            <ul>
              {items.map((record) =>
                record.kind === 'input' ? (
                  <Dependency
                    key={`${record.definition.id}:${record.definition.version}`}
                    api={api}
                    record={record}
                  />
                ) : null,
              )}
            </ul>
          )}
        </>
      ) : null}
      {dependencies.hasNextPage ? (
        <button
          type="button"
          disabled={dependencies.isFetchingNextPage}
          onClick={() => void dependencies.fetchNextPage()}
        >
          {t('integrations.moreDependencies')}
        </button>
      ) : null}
    </section>
  );
}
function Dependency({
  api,
  record,
}: {
  api: IntegrationsApi;
  record: Extract<SourceRecord, { kind: 'input' }>;
}) {
  const { t } = usePresentation();
  const [expanded, setExpanded] = useState(false);
  const input = record.definition;
  return (
    <li>
      <p>
        {t('integrations.inputVersion', { id: input.id, version: input.version })} ·{' '}
        {t(input.definition.enabled ? 'integrations.inputEnabled' : 'integrations.inputDisabled')}
      </p>
      <p>
        {t('integrations.dependencyPins', {
          connection: input.definition.connection?.version ?? 0,
          access: input.definition.connection?.access_version ?? 0,
        })}
      </p>
      <button type="button" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
        {t('integrations.showHistory')}
      </button>
      {expanded ? <ReportingHistory api={api} input={input} /> : null}
    </li>
  );
}
function ReportingHistory({
  api,
  input,
}: {
  api: IntegrationsApi;
  input: Extract<SourceRecord, { kind: 'input' }>['definition'];
}) {
  const { t } = usePresentation();
  const history = useInfiniteQuery({
    queryKey: queryKeys.integrationInputHistory(input.project_id, input.id, input.version),
    queryFn: ({ pageParam, signal }) =>
      api.getInputDiagnostics(input.project_id, input.id, input.version, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const admissions =
    history.data?.pages
      .flatMap((p) => p.admissions)
      .filter((a) => a.association.input_version === input.version) ?? [];
  return (
    <div>
      {history.isPending ? <QueryLoadingNotice label={t('integrations.showHistory')} /> : null}
      {history.error ? (
        <QueryFailureNotice
          title={t('integrations.showHistory')}
          error={safeIntegrationError(history.error, t('integrations.safeFailure'))}
          onRetry={() => history.refetch()}
        />
      ) : null}
      {history.data && !admissions.length ? (
        <QueryEmptyNotice>{t('integrations.noHistory')}</QueryEmptyNotice>
      ) : null}
      <SourceAdmissions admissions={admissions} />
      {history.hasNextPage ? (
        <button
          type="button"
          disabled={history.isFetchingNextPage}
          onClick={() => void history.fetchNextPage()}
        >
          {t('integrations.moreHistory')}
        </button>
      ) : null}
    </div>
  );
}
