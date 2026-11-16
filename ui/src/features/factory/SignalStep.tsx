import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { mergePermissions } from './flowRequirements';

type S = components['schemas'];

/** Installation owns the finite contract; authors choose inputs and routes through ordinary controls. */
export function SignalStep({
  node,
  signals,
  onChange,
  resultEditing = false,
  inputPending = false,
}: {
  node: S['FactoryFlowNodeDefinition'];
  resultEditing?: boolean;
  inputPending?: boolean;
  signals: S['FactoryDecisionSignalAuthoringContract'][];
  onChange: (
    patch: Partial<S['FactoryFlowNodeDefinition']>,
    schemas?: S['FactoryFlowDataSchema'][],
  ) => void;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<string>();
  const current =
    selected ??
    signals.find(({ contract }) => JSON.stringify(contract.action) === JSON.stringify(node.action))
      ?.alias;
  const choice = signals.find(({ alias }) => alias === current);
  return (
    <fieldset>
      <legend>{t('factory.signalPolicy')}</legend>
      <p>{t('factory.signalPolicyHelp')}</p>
      <SelectMenu
        ariaLabel={t('factory.signalPolicy')}
        value={current ?? ''}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...signals.map(({ alias, policy }) => ({
            value: alias,
            label: `${alias} · ${policy.version}`,
          })),
        ]}
        onValueChange={setSelected}
      />
      {!signals.length ? <p>{t('factory.noSignalPolicies')}</p> : null}
      {resultEditing ? <p role="status">{t('factory.signalResultEditing')}</p> : null}
      {inputPending ? <p role="status">{t('factory.signalInputPending')}</p> : null}
      <button
        type="button"
        disabled={!choice || resultEditing || inputPending}
        onClick={() => {
          if (!choice || resultEditing || inputPending) return;
          const { contract } = choice;
          onChange(
            {
              action: contract.action,
              build: null,
              gate: null,
              profile_selection: null,
              input_schema: contract.input.reference,
              outcomes: Object.entries(contract.outcomes).map(([key, schema]) => {
                const existing = node.outcomes.find((outcome) => outcome.key === key);
                // Display/disclosure settings belong to the exact accepted result contract.
                return existing &&
                  sourceReferenceKey(existing.schema) === sourceReferenceKey(schema.reference)
                  ? existing
                  : { key, kind: 'success', schema: schema.reference };
              }),
              permissions: mergePermissions(
                node.permissions,
                mergePermissions(choice.permissions, {
                  ...emptyFactoryPermissions(),
                  plugins: [contract.action.plugin],
                }),
              ),
            },
            [contract.input, ...Object.values(contract.outcomes)],
          );
        }}
      >
        {t('factory.useSignalPolicy')}
      </button>
    </fieldset>
  );
}
