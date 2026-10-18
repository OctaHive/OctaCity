import { useState } from 'react';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SourceReporting } from './SourceReporting';
import type { FactorySourcesApi } from './sourceApi';
import { sourceBindingKey, sourceReferenceKey } from './sourceReferences';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Parameters = components['schemas']['FactoryWorkStatusActionParameters'];
export type FlowSourceServices = { api: FactorySourcesApi; projectId: string };
type Props = { node: Node; services: FlowSourceServices; onChange: (node: Node) => void };
/** Configures the ordinary action; only the admitted Work supplies the native target. */
export function FlowSourceUpdate(props: Props) {
  const { t } = usePresentation();
  const parameters = props.node.action?.parameters;
  if (
    !parameters ||
    typeof parameters !== 'object' ||
    !('status' in parameters) ||
    typeof parameters.status !== 'string' ||
    !('outbox' in parameters) ||
    !parameters.outbox
  ) {
    return <p>{t('factory.deferredStatusProfile')}</p>;
  }
  return <SourceUpdateForm {...props} parameters={parameters as Parameters} />;
}
function SourceUpdateForm({
  node,
  services,
  onChange,
  parameters,
}: Props & { parameters: Parameters }) {
  const { t } = usePresentation();
  const original = parameters.source_update;
  const [connectionId, setConnectionId] = useState(original?.connection?.id ?? '');
  const connections = useInfiniteQuery({
    queryKey: queryKeys.factorySourceConnections,
    queryFn: ({ pageParam, signal }) => services.api.connections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const connection = connections.data?.pages
    .flatMap((page) => page.items)
    .find((item) => item.id === connectionId);
  const choices = useQuery({
    queryKey: queryKeys.factorySourceChoices(
      services.projectId,
      connectionId,
      connection?.version ?? 0,
    ),
    queryFn: ({ signal }) =>
      services.api.choices(services.projectId, connectionId, connection!.version, signal),
    enabled: !!connection,
  });
  const [sourceKey, setSourceKey] = useState(() =>
    original ? sourceBindingKey(original.source) : '',
  );
  const source = choices.data?.items.find(
    (item) => item.purpose === 'task_read' && sourceBindingKey(item.binding) === sourceKey,
  );
  const [writerKey, setWriterKey] = useState(() =>
    original ? sourceBindingKey(original.reporter) : '',
  );
  const writers =
    choices.data?.items.filter(
      (item) =>
        item.purpose === 'task_update' &&
        item.reporting?.editable &&
        source &&
        sourceReferenceKey(item.binding.capability) === sourceReferenceKey(node.action!.plugin) &&
        sourceReferenceKey(item.binding.installation) ===
          sourceReferenceKey(source.binding.installation) &&
        sourceReferenceKey(item.binding.resource) === sourceReferenceKey(source.binding.resource),
    ) ?? [];
  const writer = writers.find((item) => sourceBindingKey(item.binding) === writerKey);
  const [preset, setPreset] = useState('0');
  const [draft, setDraft] = useState<components['schemas']['WorkSourceStatusMapping'] | null>(
    original ? { statuses: [original.effects] } : null,
  );
  const effects = draft?.statuses[0] ?? writer?.reporting?.mapping.statuses[Number(preset)];
  const editable = writer?.reporting?.editable;
  const valid =
    connection &&
    sourceKey !== '' &&
    writerKey !== '' &&
    source &&
    writer &&
    editable &&
    effects &&
    !choices.isError &&
    !connections.isError &&
    !!effects.status.trim() &&
    effects.add.every(
      (label) => editable.labels.includes(label) && !effects.remove.includes(label),
    ) &&
    effects.remove.every((label) => editable.labels.includes(label)) &&
    (effects.state === null || editable.states.includes(effects.state));
  const select = (
    label: string,
    value: string,
    options: { value: string; label: string }[],
    change: (value: string) => void,
  ) => (
    <SelectMenu
      ariaLabel={label}
      value={value}
      options={[{ value: '', label: t('factory.chooseInput') }, ...options]}
      onValueChange={change}
    />
  );
  return (
    <section aria-label={t('factory.stageSourceUpdate')}>
      <h4>{t('factory.stageSourceUpdate')}</h4>
      <p>{t('factory.stageSourceHelp')}</p>
      <Link to="/integrations">{t('factory.manageIntegrations')}</Link>
      {connections.error || choices.error ? (
        <QueryFailureNotice
          title={t('factory.stageSourceUpdate')}
          error={connections.error ?? choices.error!}
          onRetry={() => {
            void connections.refetch();
            void choices.refetch();
          }}
        />
      ) : null}
      {select(
        t('factory.sourceConnection'),
        connectionId,
        connections.data?.pages
          .flatMap((page) => page.items)
          .map((item) => ({ value: item.id, label: item.definition.name })) ?? [],
        (value) => {
          setConnectionId(value);
          setSourceKey('');
          setWriterKey('');
          setDraft(null);
        },
      )}
      {connections.hasNextPage ? (
        <button type="button" onClick={() => void connections.fetchNextPage()}>
          {t('integrations.moreConnections')}
        </button>
      ) : null}
      {select(
        t('factory.originalTaskSource'),
        sourceKey,
        choices.data?.items.flatMap((item) =>
          item.purpose === 'task_read'
            ? [
                {
                  value: sourceBindingKey(item.binding),
                  label: `${item.locator} · ${item.binding.configuration.identity}`,
                },
              ]
            : [],
        ) ?? [],
        (value) => {
          setSourceKey(value);
          setWriterKey('');
          setDraft(null);
        },
      )}
      {select(
        t('factory.reporterMapping'),
        writerKey,
        writers.map((item) => ({
          value: sourceBindingKey(item.binding),
          label: item.binding.configuration.identity,
        })),
        (value) => {
          setWriterKey(value);
          setPreset('0');
          setDraft(null);
        },
      )}
      {writerKey !== '' && writer?.reporting ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.logicalStatus')}
            value={preset}
            options={writer.reporting.mapping.statuses.map((effect, index) => ({
              value: String(index),
              label: effect.status,
            }))}
            onValueChange={(value) => {
              setPreset(value);
              setDraft(null);
            }}
          />
          <SourceReporting
            single
            mapping={effects ? { statuses: [effects] } : undefined}
            policy={editable}
            draft={draft}
            onChange={setDraft}
          />
        </>
      ) : null}
      <button
        type="button"
        disabled={!valid}
        onClick={() => {
          if (!valid || !node.action) return;
          onChange({
            ...node,
            action: {
              ...node.action,
              parameters: {
                ...parameters,
                status: effects.status,
                source_update: {
                  source: source.binding,
                  reporter: writer.binding,
                  effects,
                  connection: {
                    id: connection.id,
                    version: connection.version,
                    access_version: choices.data!.access_version,
                    source_configuration: source.connection_configuration,
                    reporter_configuration: writer.connection_configuration,
                  },
                },
              },
            },
          });
        }}
      >
        {t('factory.useSourceEffects')}
      </button>
      {original ? (
        <SourceReporting
          mapping={{ statuses: [original.effects] }}
          policy={null}
          draft={null}
          onChange={() => {}}
        />
      ) : null}
      {original ? (
        <button
          type="button"
          onClick={() => {
            const { source_update: _removed, ...rest } = parameters;
            void _removed;
            if (node.action) onChange({ ...node, action: { ...node.action, parameters: rest } });
          }}
        >
          {t('factory.removeSourceEffects')}
        </button>
      ) : null}
    </section>
  );
}
