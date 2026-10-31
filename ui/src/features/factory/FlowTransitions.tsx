import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Root = components['schemas']['FactoryFlowRootDraft'];

/** Control edges grant no data or Context access. Both require explicit projections. */
export function FlowTransitions({
  value,
  onChange,
  repeatCeiling,
}: {
  repeatCeiling: number;
  value: Root;
  onChange: (value: Root) => void;
}) {
  const { t } = usePresentation();
  const transitionKeys = useDraftRowKeys(value.transitions);
  const update = (index: number, patch: Partial<Root['transitions'][number]>) =>
    onChange({
      ...value,
      transitions: value.transitions.map((edge, position) =>
        position === index ? { ...edge, ...patch } : edge,
      ),
    });
  return (
    <section>
      <h4>{t('factory.controlTransitions')}</h4>
      <p>{t('factory.controlOnly')}</p>
      <button
        type="button"
        disabled={value.nodes.length === 0}
        onClick={() => {
          const node = value.nodes[0];
          const terminal = value.terminals[0];
          if (!node) return;
          onChange({
            ...value,
            transitions: [
              ...value.transitions,
              {
                predecessor: node.key,
                outcome: node.outcomes[0]?.key ?? '',
                target: terminal
                  ? { kind: 'terminal', key: terminal.key }
                  : { kind: 'node', key: node.key },
                max_repeats: 0,
                exhausted_terminal: null,
              },
            ],
          });
        }}
      >
        {t('factory.addTransition')}
      </button>
      {value.transitions.map((edge, index) => (
        <fieldset key={transitionKeys[index]}>
          <legend>{t('factory.transitionNumber', { number: index + 1 })}</legend>
          <FlowRouteFields
            value={value}
            edge={edge}
            repeatCeiling={repeatCeiling}
            onChange={(patch) => update(index, patch)}
            onRemove={() =>
              onChange({
                ...value,
                transitions: value.transitions.filter((_, position) => position !== index),
              })
            }
          />
        </fieldset>
      ))}
    </section>
  );
}

/** Shared route inspector for visual selection and the complete keyboard list. */
export function FlowRouteFields({
  value,
  edge,
  repeatCeiling,
  onChange,
  onRemove,
}: {
  value: Root;
  edge: Root['transitions'][number];
  repeatCeiling: number;
  onChange: (patch: Partial<Root['transitions'][number]>) => void;
  onRemove: () => void;
}) {
  const { t } = usePresentation();
  const nodes = value.nodes.map((node) => ({ value: node.key, label: node.key }));
  const targets = [
    ...nodes.map((node) => ({
      value: `node:${node.value}`,
      label: t('factory.nodeTarget', { name: node.label }),
    })),
    ...value.terminals.map((terminal) => ({
      value: `terminal:${terminal.key}`,
      label: t('factory.terminalTarget', { name: terminal.key }),
    })),
  ];
  const update = onChange;
  return (
    <>
      <SelectMenu
        ariaLabel={t('factory.predecessor')}
        value={edge.predecessor}
        options={nodes}
        onValueChange={(predecessor) =>
          update({
            predecessor,
            outcome: value.nodes.find((node) => node.key === predecessor)?.outcomes[0]?.key ?? '',
          })
        }
      />
      <SelectMenu
        ariaLabel={t('factory.transitionOutcome')}
        value={edge.outcome}
        options={(value.nodes.find((node) => node.key === edge.predecessor)?.outcomes ?? []).map(
          (outcome) => ({ value: outcome.key, label: outcome.key }),
        )}
        onValueChange={(outcome) => update({ outcome })}
      />
      <SelectMenu
        ariaLabel={t('factory.target')}
        value={`${edge.target.kind}:${edge.target.key}`}
        options={targets}
        onValueChange={(selected) => {
          const separator = selected.indexOf(':');
          const kind = selected.slice(0, separator);
          if (kind === 'node' || kind === 'terminal')
            update({ target: { kind, key: selected.slice(separator + 1) } });
        }}
      />
      <label>
        {t('factory.repeatBound')}
        <input
          type="number"
          min={0}
          max={repeatCeiling}
          value={edge.max_repeats}
          onChange={(event) => update({ max_repeats: Number(event.target.value) })}
        />
      </label>
      <SelectMenu
        ariaLabel={t('factory.exhaustion')}
        value={edge.exhausted_terminal ?? ''}
        options={[
          { value: '', label: t('factory.none') },
          ...value.terminals.map((terminal) => ({ value: terminal.key, label: terminal.key })),
        ]}
        onValueChange={(selected) => update({ exhausted_terminal: selected || null })}
      />
      <label>
        <input
          type="checkbox"
          checked={edge.starts_cycle ?? false}
          onChange={(event) => update({ starts_cycle: event.target.checked })}
        />
        {t('factory.startsCycle')}
      </label>
      <button type="button" onClick={onRemove}>
        {t('factory.removeTransition')}
      </button>
    </>
  );
}
