import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { flowLabel } from './flowLabels';

type S = components['schemas'];
type Draft = S['FactoryFlowDefinitionDraft'];
type Flow = S['FactoryFlowDefinitionDetails'];
type Handoff = S['FactoryFlowAcceptedPredecessor'];

const completionKey = ({ definition, terminal }: Pick<Handoff, 'definition' | 'terminal'>) =>
  JSON.stringify([definition.id, definition.version, definition.digest, terminal]);

/** The exact published completion supplies the selectable incoming schema, never an automatic edge. */
export function FlowIncomingCompletion({
  value,
  flows,
  onChange,
}: {
  value: Draft;
  flows: readonly Flow[];
  onChange: (draft: Draft) => void;
}) {
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
  if (!choices.length && !current) return null;
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
