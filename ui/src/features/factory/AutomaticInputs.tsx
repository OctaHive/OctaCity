import { SourceGroupEditor } from './SourceGroupEditor';
import { useState } from 'react';
import { SourceInputEditor, type SourceRepositories } from './SourceInputEditor';
import { useInfiniteQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { FactoryConfigurationDetails } from './api';
import type { FactorySourcesApi } from './sourceApi';

/** Automatic input administration is loaded only when explicitly opened. */
export function AutomaticInputs({
  configuration,
  api,
  repositories,
}: {
  configuration: FactoryConfigurationDetails;
  api: FactorySourcesApi;
  repositories?: SourceRepositories | undefined;
}) {
  const { t } = usePresentation();
  const [editing, setEditing] = useState<string | null>(null);
  const project = configuration.configuration.reference.project_id;
  const connections = useInfiniteQuery({
    queryKey: queryKeys.factorySourceConnections,
    queryFn: ({ pageParam, signal }) => api.connections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const inputs = useInfiniteQuery({
    queryKey: queryKeys.factorySourceInputs(project),
    queryFn: ({ pageParam, signal }) => api.list(project, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  async function refresh() {
    const [updated, metadata] = await Promise.all([
      inputs.refetch({ throwOnError: true }),
      connections.refetch({ throwOnError: true }),
    ]);
    return {
      records: updated.data?.pages.flatMap((page) => page.items) ?? [],
      connections: metadata.data?.pages.flatMap((page) => page.items) ?? [],
    };
  }
  const records = inputs.data?.pages.flatMap((page) => page.items) ?? [];
  const available = connections.data?.pages.flatMap((page) => page.items) ?? [];
  const selected = records.find((record) => record.definition.id === editing);
  const groupInputs = records.flatMap((record) =>
    record.kind === 'input' ? [record.definition] : [],
  );
  const isGroup = editing === 'group' || selected?.kind === 'group';
  return (
    <section aria-labelledby="automatic-intake-heading">
      <h4 id="automatic-intake-heading">{t('factory.automaticInputs')}</h4>
      <Link to="/integrations">{t('factory.manageIntegrations')}</Link>
      {connections.isPending || inputs.isPending ? (
        <QueryLoadingNotice label={t('factory.automaticInputs')} />
      ) : null}
      {connections.error || inputs.error ? (
        <QueryFailureNotice
          title={t('factory.automaticInputs')}
          error={new Error(t('factory.sourceFailure'))}
          onRetry={() => Promise.all([connections.refetch(), inputs.refetch()])}
        />
      ) : null}
      <button
        type="button"
        disabled={!repositories || !available.length || !!connections.error || !!inputs.error}
        onClick={() => setEditing('new')}
      >
        {t('factory.addInput')}
      </button>
      <button
        type="button"
        disabled={groupInputs.length < 2 || !!inputs.error}
        onClick={() => setEditing('group')}
      >
        {t('factory.addGroup')}
      </button>
      {editing && (repositories || isGroup) ? (
        <>
          <button type="button" onClick={() => setEditing(null)}>
            {t('factory.closeInput')}
          </button>
          {isGroup ? (
            <SourceGroupEditor
              key={editing}
              project={project}
              api={api}
              inputs={groupInputs}
              group={selected?.kind === 'group' ? selected.definition : undefined}
              onRefresh={refresh}
            />
          ) : repositories ? (
            <SourceInputEditor
              key={editing}
              configuration={configuration}
              api={api}
              connections={available}
              repositories={repositories}
              input={selected?.kind === 'input' ? selected.definition : undefined}
              onRefresh={refresh}
            />
          ) : null}
        </>
      ) : null}
      <ul>
        {records
          .filter(
            (record) =>
              record.kind === 'input' &&
              record.definition.definition.target.id === configuration.configuration.reference.id,
          )
          .map((record) =>
            record.kind === 'input' ? (
              <li key={record.definition.id}>
                {record.definition.definition.source.resource.identity} ·{' '}
                {record.definition.definition.source.configuration.identity} · v
                {record.definition.version} ·{' '}
                {t(
                  record.definition.definition.enabled
                    ? 'factory.inputEnabled'
                    : 'factory.inputDisabled',
                )}{' '}
                <button type="button" onClick={() => setEditing(record.definition.id)}>
                  {t('factory.editInput')}
                </button>
              </li>
            ) : null,
          )}
      </ul>
      <ul>
        {records.map((record) =>
          record.kind === 'group' ? (
            <li key={record.definition.id}>
              {t('factory.exclusiveGroup')} · v{record.definition.version}{' '}
              <button type="button" onClick={() => setEditing(record.definition.id)}>
                {t('factory.editGroup')}
              </button>
            </li>
          ) : null,
        )}
      </ul>
      {inputs.hasNextPage ? (
        <button
          type="button"
          disabled={inputs.isFetchingNextPage}
          onClick={() => void inputs.fetchNextPage()}
        >
          {t('factory.moreInputs')}
        </button>
      ) : null}
      {connections.hasNextPage ? (
        <button
          type="button"
          disabled={connections.isFetchingNextPage}
          onClick={() => void connections.fetchNextPage()}
        >
          {t('factory.moreConnections')}
        </button>
      ) : null}
      {connections.data?.pages.every((page) => !page.items.length) ? (
        <QueryEmptyNotice>{t('factory.noSourceSetup')}</QueryEmptyNotice>
      ) : null}
    </section>
  );
}
