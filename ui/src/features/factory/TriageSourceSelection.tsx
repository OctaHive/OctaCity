import { useState } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { sourceBindingKey } from '../../shared/sourceReferences';
import type { FactoryApi } from './api';
import type { FactoryWorkSources } from './FactoryWorkForm';
import { SourceInputEditor } from './SourceInputEditor';
import type { FactorySourcesApi, SourceIntent, InputChoice } from './sourceApi';

type Schema = components['schemas'];
export type TriageSourceDraft = {
  input: Schema['FactoryPresetInputSelection'];
  repository: NonNullable<Schema['FactoryPresetResolutionRequest']['repository']>;
  writer: InputChoice | undefined;
};
/** Resolves only an ordinary validation preview; Source publication/preview waits for the committed target. */
export function TriageSourceSelection({
  api,
  sources,
  workSources,
  request,
  onSelect,
}: {
  api: FactoryApi;
  sources: FactorySourcesApi;
  workSources: FactoryWorkSources;
  request: Schema['CreateFactoryConfigurationRequest'];
  onSelect: (value: TriageSourceDraft) => void;
}) {
  const { t } = usePresentation();
  const [preview, setPreview] = useState<{
    signature: string;
    configuration: Schema['FactoryConfigurationDocument'];
  } | null>(null);
  const [mappingProblem, setMappingProblem] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const signature = JSON.stringify(request);
  const connections = useInfiniteQuery({
    queryKey: queryKeys.factorySourceConnections,
    queryFn: ({ pageParam, signal }) => sources.connections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const available = connections.data?.pages.flatMap((page) => page.items) ?? [];
  async function review() {
    setPending(true);
    setError(null);
    try {
      const result = await api.validateConfiguration(request);
      setPreview({ signature, configuration: result.configuration });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  function select(intent: SourceIntent, choices: Schema['WorkSourceInputChoices']) {
    const selected = intent.inputs[0];
    const definition = selected?.definition;
    if (!selected || !definition?.connection) return;
    const read = choices.items.find(
      (item) =>
        item.purpose === 'task_read' &&
        sourceBindingKey(item.binding) === sourceBindingKey(definition.source),
    );
    if (
      read?.preparation?.kind !== 'process_completion' ||
      read.preparation.value.project_id !== request.project_id
    ) {
      setMappingProblem(true);
      return;
    }
    setMappingProblem(false);
    onSelect({
      input: {
        id: selected.id,
        connection: definition.connection,
        source: definition.source,
        reporter: definition.reporter ?? null,
        predicate: definition.predicate ?? null,
        reporting: definition.reporting ?? null,
      },
      repository: definition.repository,
      writer: choices.items.find(
        (item) =>
          item.purpose === 'task_update' &&
          definition.reporter &&
          sourceBindingKey(item.binding) === sourceBindingKey(definition.reporter),
      ),
    });
  }
  return (
    <section aria-label={t('triage.sourceSelection')}>
      <h3>{t('triage.sourceSelection')}</h3>
      <button type="button" disabled={pending} onClick={() => void review()}>
        {t('triage.reviewSource')}
      </button>
      {mappingProblem ? <p role="alert">{t('triage.processMappingRequired')}</p> : null}
      {error || connections.error ? (
        <QueryFailureNotice
          title={t('triage.sourceSelection')}
          error={error ?? connections.error}
          onRetry={() => {
            setError(null);
            void connections.refetch();
          }}
        />
      ) : null}
      {connections.hasNextPage ? (
        <button
          type="button"
          disabled={connections.isFetchingNextPage}
          onClick={() => void connections.fetchNextPage()}
        >
          {t('factory.moreChoices')}
        </button>
      ) : null}
      {preview ? (
        <fieldset disabled={preview.signature !== signature || connections.isError}>
          {preview.signature !== signature ? <p>{t('triage.sourcePreviewStale')}</p> : null}
          <SourceInputEditor
            key={preview.signature}
            configuration={{ configuration: preview.configuration }}
            api={sources}
            connections={available}
            repositories={workSources}
            onSelect={select}
            onRefresh={async () => ({ records: [], connections: available })}
          />
        </fieldset>
      ) : null}
    </section>
  );
}
