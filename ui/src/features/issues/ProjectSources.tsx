import { useRef, useState } from 'react';
import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { sourceFailure } from '../../shared/sourceErrors';
import { sourceBindingKey, sourceReferenceKey } from '../../shared/sourceReferences';
import { ManagementApiError } from '../../api/client';
import type { IssueSourcesApi, IssueSourceImport, IssueSourceImportIntent } from './sourceApi';
import { ProjectSourceEditor } from './ProjectSourceEditor';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import styles from './Issues.module.css';

/** Project import is independently reviewed before any task or execution exists. */
export function ProjectSources({ projectId, api }: { projectId: string; api: IssueSourcesApi }) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [preparing, setPreparing] = useState(false);
  const [reviewFailure, setReviewFailure] = useState<{
    error: Error;
    retry: () => Promise<void>;
  } | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const [lifecycle, setLifecycle] = useState<{
    intent: IssueSourceImportIntent;
    summary: string;
    returnFocus: HTMLElement;
  } | null>(null);
  const connections = useInfiniteQuery({
    queryKey: queryKeys.issueImportConnections,
    queryFn: ({ pageParam, signal }) => api.connections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const imports = useInfiniteQuery({
    queryKey: queryKeys.issueImports(projectId),
    queryFn: ({ pageParam, signal }) => api.imports(projectId, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const available = connections.data?.pages.flatMap((page) => page.items) ?? [];
  async function reviewLifecycle(record: IssueSourceImport, returnFocus: HTMLElement) {
    setPreparing(true);
    setReviewFailure(null);
    try {
      const definition = structuredClone(record.definition);
      definition.enabled = !definition.enabled;
      if (definition.enabled) {
        const choices = await api.importChoices(
          projectId,
          definition.connection.id,
          definition.connection.version,
        );
        if (
          !choices.items.some(
            (choice) =>
              choice.purpose === 'task_read' &&
              sourceBindingKey(choice.binding) === sourceBindingKey(definition.source) &&
              sourceReferenceKey(choice.connection_configuration) ===
                sourceReferenceKey(definition.connection.source_configuration),
          )
        )
          throw new ManagementApiError({
            code: 'capability_unavailable',
            message: '',
            status: null,
            requestId: null,
            retryAfterMilliseconds: null,
          });
        // Mutable grants may advance; immutable connection and Source pins must still match.
        definition.connection.access_version = choices.access_version;
      }
      const predicate = definition.predicate;
      setLifecycle({
        intent: {
          id: record.id,
          project_id: projectId,
          expected_current_version: record.version,
          definition,
        },
        returnFocus,
        summary: [
          available.find((item) => item.id === definition.connection.id)?.definition.name ??
            definition.source.installation.identity,
          `${definition.source.resource.identity} · ${definition.source.configuration.identity} / ${definition.source.configuration.version}`,
          t('issues.revision', { version: record.version }),
          predicate
            ? [
                `${t('factory.taskState')}: ${predicate.state ?? t('factory.anyState')}`,
                `${t('factory.allLabels')}: ${predicate.all_of.join(', ')}`,
                `${t('factory.anyLabels')}: ${predicate.any_of.join(', ')}`,
                `${t('factory.excludedLabels')}: ${predicate.none_of.join(', ')}`,
              ].join('; ')
            : t('factory.installedFilters'),
          t(definition.enabled ? 'issueImport.active' : 'issueImport.inactive'),
          t('issueImport.lifecycleHelp'),
        ].join('; '),
      });
    } catch (error) {
      setReviewFailure({
        error: sourceFailure(error, t),
        retry: () => reviewLifecycle(record, returnFocus),
      });
    } finally {
      setPreparing(false);
    }
  }
  async function publishLifecycle(attempt: ConfirmedMutationAttempt<IssueSourceImportIntent>) {
    try {
      return await api.publishImport(
        structuredClone(attempt.request) as IssueSourceImportIntent,
        attempt.headers,
      );
    } catch (error) {
      throw sourceFailure(error, t);
    }
  }
  return (
    <section className={styles.panel} aria-label={t('issueImport.open')}>
      <h2>{t('issueImport.open')}</h2>
      <p>{t('issueImport.help')}</p>
      <Link to="/integrations">{t('factory.manageIntegrations')}</Link>
      <Link to={`/projects/${projectId}/issue-workflows`}>{t('issues.workflow')}</Link>
      {connections.isPending || imports.isPending ? (
        <QueryLoadingNotice label={t('issueImport.open')} />
      ) : null}
      {connections.error || imports.error ? (
        <QueryFailureNotice
          title={t('issueImport.open')}
          error={sourceFailure(connections.error ?? imports.error, t)}
          onRetry={() => Promise.all([connections.refetch(), imports.refetch()])}
        />
      ) : null}
      {connections.isSuccess && !available.length ? <p>{t('factory.noSourceSetup')}</p> : null}
      {preparing ? <QueryLoadingNotice label={t('issueImport.publish')} /> : null}
      {reviewFailure ? (
        <QueryFailureNotice
          title={t('issueImport.publish')}
          error={reviewFailure.error}
          onRetry={reviewFailure.retry}
        />
      ) : null}
      <button
        type="button"
        ref={addRef}
        disabled={!available.length || !!connections.error || !!imports.error}
        onClick={() => setAdding(!adding)}
      >
        {t(adding ? 'command.abandon' : 'issueImport.add')}
      </button>
      {adding ? (
        <ProjectSourceEditor
          projectId={projectId}
          api={api}
          connections={available}
          ready={!connections.isFetching && !connections.error && !imports.error}
          onClose={() => setAdding(false)}
          fallbackFocusRef={addRef}
          onRefresh={() => Promise.all([connections.refetch(), imports.refetch()])}
        />
      ) : null}
      <ul>
        {imports.data?.pages
          .flatMap((page) => page.items)
          .map((record) => (
            <li key={record.id}>
              {record.definition.source.resource.identity} ·{' '}
              {record.definition.source.configuration.identity} ·{' '}
              {t('issues.revision', { version: record.version })}
              <p>{t(record.definition.enabled ? 'issueImport.active' : 'issueImport.inactive')}</p>
              <button
                type="button"
                disabled={imports.isFetching || !!imports.error || lifecycle !== null || preparing}
                onClick={(event) => void reviewLifecycle(record, event.currentTarget)}
              >
                {t(record.definition.enabled ? 'issueImport.disable' : 'issueImport.enable')}
              </button>
            </li>
          ))}
      </ul>
      {imports.hasNextPage ? (
        <button
          type="button"
          onClick={() => void imports.fetchNextPage()}
          disabled={imports.isFetchingNextPage}
        >
          {t('issueImport.more')}
        </button>
      ) : null}
      {lifecycle ? (
        <ConfirmedCommand
          title={t('issueImport.publish')}
          confirmLabel={t('issueImport.confirm')}
          request={lifecycle.intent}
          consequence={lifecycle.summary}
          returnFocus={lifecycle.returnFocus}
          fallbackFocusRef={addRef}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.issueImports(projectId) }]}
          execute={publishLifecycle}
          onRefresh={() => imports.refetch()}
          onClose={() => setLifecycle(null)}
          renderSuccess={() => <p>{t('issueImport.published')}</p>}
        />
      ) : null}
      {connections.hasNextPage ? (
        <button
          type="button"
          onClick={() => void connections.fetchNextPage()}
          disabled={connections.isFetchingNextPage}
        >
          {t('factory.moreConnections')}
        </button>
      ) : null}
    </section>
  );
}
