import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import type { FactoryApi } from './api';
import { flowLabel } from './flowLabels';
import { VersionPicker } from './VersionPicker';

type S = components['schemas'];
type Draft = S['FactoryFlowDefinitionDraft'];
type Flow = S['FactoryFlowDefinitionDetails'];
type Handoff = S['FactoryFlowAcceptedPredecessor'];

const completionKey = ({ definition, terminal }: Pick<Handoff, 'definition' | 'terminal'>) =>
  JSON.stringify([definition.id, definition.version, definition.digest, terminal]);

type Props = {
  value: Draft;
  flows: readonly Flow[];
  onChange: (draft: Draft) => void;
  empty?: string;
  flowApi?: Pick<FactoryApi, 'getFlow'> | undefined;
};

/** The exact published completion supplies the selectable incoming schema, never an automatic edge. */
export function FlowIncomingCompletion({ flowApi, ...props }: Props) {
  return flowApi ? <VersionedCompletion {...props} api={flowApi} /> : <Completion {...props} />;
}

function VersionedCompletion({ api, ...props }: Props & { api: Pick<FactoryApi, 'getFlow'> }) {
  const { t } = usePresentation();
  const { value, flows } = props;
  const [selection, setSelection] = useState<{ id: string; version: number }>();
  const pinned = value.accepted_predecessor?.definition;
  const previous = flows.filter(({ flow }) => flow.closure.root.version > 1);
  const selected = previous.find(({ flow }) => flow.closure.root.id === selection?.id);
  const needsLoad = (reference: { id: string; version: number } | undefined) =>
    reference &&
    !flows.some(
      ({ flow }) =>
        flow.closure.root.id === reference.id && flow.closure.root.version === reference.version,
    );
  const pinnedQuery = useQuery({
    queryKey: queryKeys.factoryFlow(pinned?.id ?? '', pinned?.version ?? 0),
    queryFn: ({ signal }) => api.getFlow(pinned!.id, pinned!.version, signal),
    enabled: needsLoad(pinned) === true,
  });
  const historyQuery = useQuery({
    queryKey: queryKeys.factoryFlow(selection?.id ?? '', selection?.version ?? 0),
    queryFn: ({ signal }) => api.getFlow(selection!.id, selection!.version, signal),
    enabled: needsLoad(selection) === true,
  });
  const versions = [...flows];
  for (const detail of [pinnedQuery.data, historyQuery.data]) {
    if (
      detail &&
      !versions.some(
        ({ flow }) =>
          flow.closure.root.id === detail.flow.closure.root.id &&
          flow.closure.root.version === detail.flow.closure.root.version,
      )
    ) {
      versions.push(detail);
    }
  }
  return (
    <>
      {previous.length ? (
        <details>
          <summary>{t('factory.previousFlowVersion')}</summary>
          <SelectMenu
            ariaLabel={t('factory.flows')}
            value={selection?.id ?? ''}
            options={previous.map(({ flow }) => ({
              value: flow.closure.root.id,
              label: flowLabel(flow, t('factory.immutableFlow')),
            }))}
            onValueChange={(id) => {
              const flow = previous.find(({ flow }) => flow.closure.root.id === id);
              if (flow) setSelection(flow.flow.closure.root);
            }}
          />
          {selected && selection ? (
            <VersionPicker
              current={selected.flow.closure.root.version}
              selected={selection.version}
              onChange={(version) => setSelection({ id: selection.id, version })}
            />
          ) : null}
          {historyQuery.isFetching ? <QueryLoadingNotice label={t('factory.version')} /> : null}
          {historyQuery.isError ? (
            <QueryFailureNotice
              title={t('factory.version')}
              error={historyQuery.error}
              onRetry={historyQuery.refetch}
            />
          ) : null}
        </details>
      ) : null}
      <Completion {...props} flows={versions} />
    </>
  );
}

function Completion({ value, flows, onChange, empty }: Props) {
  const { t } = usePresentation();
  const current = value.accepted_predecessor;
  const choices = flows.flatMap(({ flow }) => {
    const root = flow.closure.definitions.find(
      ({ reference }) =>
        reference.id === flow.closure.root.id &&
        reference.version === flow.closure.root.version &&
        reference.digest === flow.closure.root.digest,
    );
    if (!root) return [];
    return root.terminals.flatMap(({ key: terminal, schema }) => {
      const data = flow.data_schemas.find(
        ({ reference }) => sourceReferenceKey(reference) === sourceReferenceKey(schema),
      );
      const outcomes = root.transitions
        .filter(
          (edge) =>
            (edge.target.kind === 'terminal' && edge.target.key === terminal) ||
            edge.exhausted_terminal === terminal,
        )
        .flatMap(
          (edge) =>
            root.nodes
              .find(({ key }) => key === edge.predecessor)
              ?.outcomes.filter(
                ({ key, kind, schema: result }) =>
                  key === edge.outcome &&
                  kind === 'success' &&
                  sourceReferenceKey(result) === sourceReferenceKey(schema),
              ) ?? [],
        );
      if (!data || !outcomes.length) return [];
      const pin = { definition: root.reference, terminal, outcomes: [] };
      return [
        {
          pin,
          data,
          key: completionKey(pin),
          outcomes: [...new Set(outcomes.map(({ key }) => key))],
          label: `${flowLabel(flow, t('factory.immutableFlow'))} · ${terminal}`,
        },
      ];
    });
  });
  const currentKey = current ? completionKey(current) : '';
  const selected = choices.find(({ key }) => key === currentKey);
  if (!choices.length && !current) return empty ? <p>{empty}</p> : null;
  return (
    <fieldset>
      <legend>{t('factory.incomingCompletion')}</legend>
      <p>{t('factory.incomingCompletionHelp')}</p>
      <SelectMenu
        ariaLabel={t('factory.incomingCompletion')}
        value={currentKey}
        options={[
          { value: '', label: t('factory.none') },
          ...choices.map(({ key, label }) => ({ value: key, label })),
          ...(current && !selected ? [{ value: currentKey, label: current.terminal }] : []),
        ]}
        onValueChange={(key) => {
          const choice = choices.find((choice) => choice.key === key);
          if (choice) {
            const existing = value.data_schemas.some(
              ({ reference }) =>
                sourceReferenceKey(reference) === sourceReferenceKey(choice.data.reference),
            );
            onChange({
              ...value,
              accepted_predecessor: choice.pin,
              data_schemas: existing ? value.data_schemas : [...value.data_schemas, choice.data],
            });
          } else if (!key) {
            const { accepted_predecessor: _removed, ...rest } = value;
            void _removed;
            onChange(rest);
          }
        }}
      />
      {current &&
        selected?.outcomes.map((outcome) => (
          <label key={outcome}>
            <input
              type="checkbox"
              checked={current.outcomes.includes(outcome)}
              onChange={(event) =>
                onChange({
                  ...value,
                  accepted_predecessor: {
                    ...current,
                    outcomes: event.target.checked
                      ? [...current.outcomes, outcome].sort()
                      : current.outcomes.filter((key) => key !== outcome),
                  },
                })
              }
            />
            {outcome}
          </label>
        ))}
    </fieldset>
  );
}
