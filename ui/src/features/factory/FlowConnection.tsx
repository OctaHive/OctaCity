import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Root = components['schemas']['FactoryFlowRootDraft'];
type Edge = Root['transitions'][number];
/** Declares only control flow. Data and Context access are separate operator choices. */
export function FlowConnection({
  root,
  source,
  target,
  onTarget,
  onAdd,
  onCancel,
}: {
  root: Root;
  source: number;
  target: Edge['target'] | null;
  onTarget: (target: Edge['target']) => void;
  onAdd: (edge: Edge) => void;
  onCancel: () => void;
}) {
  const { t } = usePresentation();
  const node = root.nodes[source];
  const [selectedOutcome, setSelectedOutcome] = useState('');
  const outcome =
    node?.outcomes.find((item) => item.key === selectedOutcome) ??
    (node?.outcomes.length === 1 ? node.outcomes[0] : undefined);
  const targets = [
    ...root.nodes
      .filter((_, index) => index !== source && !!_.key)
      .map((item) => ({
        value: `node:${item.key}`,
        label: t('factory.nodeTarget', { name: item.key }),
      })),
    ...root.terminals.map((item) => ({
      value: `terminal:${item.key}`,
      label: t('factory.terminalTarget', { name: item.key }),
    })),
  ];
  const targetKey = target ? `${target.kind}:${target.key}` : '';
  const duplicate = root.transitions.some(
    (edge) =>
      edge.predecessor === node?.key &&
      edge.outcome === outcome?.key &&
      edge.target.kind === target?.kind &&
      edge.target.key === target?.key,
  );
  return (
    <fieldset>
      <legend>{t('factory.newGraphRoute')}</legend>
      <p>{t('factory.newGraphRouteHelp', { name: node?.key ?? t('factory.unnamedNode') })}</p>
      <SelectMenu
        ariaLabel={t('factory.transitionOutcome')}
        value={outcome?.key ?? ''}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...(node?.outcomes ?? []).map((item) => ({ value: item.key, label: item.key })),
        ]}
        onValueChange={setSelectedOutcome}
      />
      <SelectMenu
        ariaLabel={t('factory.target')}
        value={targetKey}
        options={[{ value: '', label: t('factory.chooseInput') }, ...targets]}
        onValueChange={(value) => {
          const kind = value.slice(0, value.indexOf(':'));
          if (kind === 'node' || kind === 'terminal')
            onTarget({ kind, key: value.slice(value.indexOf(':') + 1) });
        }}
      />
      <p>{t('factory.controlOnly')}</p>
      {duplicate ? <p role="alert">{t('factory.routeAlreadyExists')}</p> : null}
      <button
        type="button"
        disabled={
          !node ||
          !outcome ||
          !target ||
          !targets.some((item) => item.value === targetKey) ||
          duplicate
        }
        onClick={() => {
          if (node && outcome && target)
            onAdd({
              predecessor: node.key,
              outcome: outcome.key,
              target,
              max_repeats: 0,
              exhausted_terminal: null,
            });
        }}
      >
        {t('factory.addGraphRoute')}
      </button>
      <button type="button" onClick={onCancel}>
        {t('factory.cancelGraphConnection')}
      </button>
    </fieldset>
  );
}
