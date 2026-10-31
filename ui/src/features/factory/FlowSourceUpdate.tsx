import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SourceReporting } from '../../shared/SourceReporting';
import type { FactorySourcesApi } from './sourceApi';
import { sourceBindingKey, sourceReferenceKey } from '../../shared/sourceReferences';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Parameters = components['schemas']['FactoryWorkStatusActionParameters'];
export type FlowSourceServices = { api: FactorySourcesApi; projectId: string };
export type SourceUpdateDraft = {
  attempts: number;
  delay: number;
  connectionId: string;
  connectionVersion: number | undefined;
  sourceKey: string;
  writerKey: string;
  writerPin: string | null;
  preset: string;
  effects: components['schemas']['WorkSourceStatusMapping'] | null;
};
type Props = {
  draft?: SourceUpdateDraft | undefined;
  onDraftChange: (draft: SourceUpdateDraft | undefined) => void;
  node: Node;
  services: FlowSourceServices;
  onChange: (node: Node, schemas?: components['schemas']['FactoryFlowDataSchema'][]) => void;
};
/** Configures the ordinary action; only the admitted Work supplies the native target. */
export function FlowSourceUpdate(props: Props) {
  const { t } = usePresentation();
  if (!props.node.action) return <SourceUpdateForm {...props} parameters={null} />;
  const parameters = props.node.action.parameters;
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
  draft: editing,
  onDraftChange,
}: Props & { parameters: Parameters | null }) {
  const { t } = usePresentation();
  const original = parameters?.source_update;
  const form: SourceUpdateDraft = editing ?? {
    attempts: parameters?.outbox?.max_attempts ?? 1,
    delay: parameters?.outbox?.retry_delay_millis ?? 0,
    connectionId: original?.connection?.id ?? '',
    connectionVersion: original?.connection?.version,
    sourceKey: original ? sourceBindingKey(original.source) : '',
    writerKey: original ? sourceBindingKey(original.reporter) : '',
    writerPin: null,
    preset: '0',
    effects: original ? { statuses: [original.effects] } : null,
  };
  const {
    attempts,
    delay,
    connectionId,
    connectionVersion,
    sourceKey,
    writerKey,
    writerPin,
    preset,
    effects: draft,
  } = form;
  const edit = (patch: Partial<SourceUpdateDraft>) => onDraftChange({ ...form, ...patch });
  const connections = useInfiniteQuery({
    queryKey: queryKeys.factorySourceConnections,
    queryFn: ({ pageParam, signal }) => services.api.connections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const connection = connections.data?.pages
    .flatMap((page) => page.items)
    .find((item) => item.id === connectionId && item.version === connectionVersion);
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
  const source = choices.data?.items.find(
    (item) => item.purpose === 'task_read' && sourceBindingKey(item.binding) === sourceKey,
  );
  const writers =
    choices.data?.items.filter(
      (item) =>
        item.purpose === 'task_update' &&
        item.reporting?.editable &&
        source &&
        (node.action
          ? sourceReferenceKey(item.binding.capability) === sourceReferenceKey(node.action.plugin)
          : item.reporting.action &&
            sourceReferenceKey(item.reporting.action.plugin) ===
              sourceReferenceKey(item.binding.capability)) &&
        sourceReferenceKey(item.binding.installation) ===
          sourceReferenceKey(source.binding.installation) &&
        sourceReferenceKey(item.binding.resource) === sourceReferenceKey(source.binding.resource),
    ) ?? [];
  const writer = writers.find((item) => sourceBindingKey(item.binding) === writerKey);
  const offered = writer?.reporting?.action;
  const writerSnapshot =
    writer && choices.data
      ? JSON.stringify([
          choices.data.access_version,
          writer.connection_configuration,
          writer.reporting,
        ])
      : null;
  const staleWriter = writerPin !== null && writerPin !== writerSnapshot;
  const contract =
    offered &&
    sourceReferenceKey(offered.plugin) === sourceReferenceKey(writer.binding.capability) &&
    sourceReferenceKey(offered.output.reference) === sourceReferenceKey(offered.outcome.schema)
      ? offered
      : null;
  const conflict =
    !node.action &&
    contract &&
    node.outcomes.some(
      (outcome) =>
        outcome.key === contract.outcome.key &&
        sourceReferenceKey(outcome.schema) !== sourceReferenceKey(contract.outcome.schema),
    );
  const effects = draft?.statuses[0] ?? writer?.reporting?.mapping.statuses[Number(preset)];
  const editable = writer?.reporting?.editable;
  const valid =
    (node.action || contract) &&
    !conflict &&
    !staleWriter &&
    Number.isSafeInteger(attempts) &&
    attempts > 0 &&
    Number.isSafeInteger(delay) &&
    delay >= 0 &&
    (!contract ||
      (attempts <= contract.parameters.max_attempts &&
        delay <= contract.parameters.max_retry_delay_millis)) &&
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
          edit({
            connectionId: value,
            connectionVersion: connections.data?.pages
              .flatMap((page) => page.items)
              .find((item) => item.id === value)?.version,
            writerPin: null,
            sourceKey: '',
            writerKey: '',
            effects: null,
          });
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
          edit({ sourceKey: value, writerKey: '', writerPin: null, effects: null });
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
          const chosen = writers.find((item) => sourceBindingKey(item.binding) === value);
          edit({
            writerKey: value,
            writerPin:
              chosen && choices.data
                ? JSON.stringify([
                    choices.data.access_version,
                    chosen.connection_configuration,
                    chosen.reporting,
                  ])
                : null,
            preset: '0',
            effects: null,
          });
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
              edit({ preset: value, effects: null });
            }}
          />
          <SourceReporting
            single
            mapping={effects ? { statuses: [effects] } : undefined}
            policy={editable}
            draft={draft}
            onChange={(effects) => edit({ effects })}
          />
        </>
      ) : null}
      {contract ? (
        <fieldset>
          <legend>{t('factory.reportDeliveryLimits')}</legend>
          <label>
            {t('factory.reportDeliveryAttempts')}
            <input
              type="number"
              min={1}
              max={contract.parameters.max_attempts}
              value={attempts}
              onChange={(event) => edit({ attempts: Number(event.target.value) })}
            />
          </label>
          <label>
            {t('factory.reportRetryDelay')}
            <input
              type="number"
              min={0}
              max={contract.parameters.max_retry_delay_millis}
              value={delay}
              onChange={(event) => edit({ delay: Number(event.target.value) })}
            />
          </label>
        </fieldset>
      ) : null}
      {staleWriter ? <p role="alert">{t('factory.staleReporterChoices')}</p> : null}
      {connectionId && !connection ? (
        <p role="alert">{t('factory.staleSourceConnection')}</p>
      ) : null}
      {conflict ? (
        <p role="alert">{t('factory.actionOutcomeConflict', { name: contract!.outcome.key })}</p>
      ) : null}
      <button
        type="button"
        disabled={!valid}
        onClick={() => {
          if (!valid) return;
          const action =
            node.action ??
            (contract
              ? { action: contract.action, plugin: contract.plugin, parameters: {} }
              : null);
          if (!action) return;
          onDraftChange(undefined);
          onChange(
            {
              ...node,
              ...(!node.action && contract
                ? {
                    outcomes: [
                      ...node.outcomes.filter((outcome) => outcome.key !== contract.outcome.key),
                      contract.outcome,
                    ],
                  }
                : {}),
              action: {
                ...action,
                parameters: {
                  ...parameters,
                  outbox: { max_attempts: attempts, retry_delay_millis: delay },
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
            },
            !node.action && contract ? [contract.output] : undefined,
          );
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
            const { source_update: _removed, ...rest } = parameters!;
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
