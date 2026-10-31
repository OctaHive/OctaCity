import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];

/** Chooses a declared ordinary compiler; command arguments and task identity remain installation-owned. */
export function CommandStep({
  node,
  commands,
  onChange,
}: {
  node: Node;
  commands: Schema['FactoryAuthoringCapabilities']['commands'];
  onChange: (patch: Partial<Node>) => void;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] =
    useState<Schema['FactoryAuthoringCapabilities']['commands'][number]>();
  const [outcomeKey, setOutcomeKey] = useState('');
  const commandIndex = selected
    ? commands.findIndex((item) => JSON.stringify(item) === JSON.stringify(selected))
    : -1;
  const command = commandIndex < 0 ? undefined : selected;
  const outcomes = node.outcomes.filter((item) => item.kind === 'success');
  const outcome =
    outcomes.find((item) => item.key === outcomeKey) ??
    (outcomes.length === 1 ? outcomes[0] : undefined);
  return (
    <fieldset>
      <legend>{t('factory.commandStep')}</legend>
      <p>{t('factory.commandStepHelp')}</p>
      {!commands.length ? <p role="alert">{t('factory.noInstalledCommand')}</p> : null}
      <SelectMenu
        ariaLabel={t('factory.installedCommand')}
        value={commandIndex < 0 ? '' : String(commandIndex)}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...commands.map((item, index) => ({
            value: String(index),
            label: `${item.alias} · v${item.execution.build_configuration.version}`,
          })),
        ]}
        onValueChange={(index) =>
          setSelected(index === '' ? undefined : structuredClone(commands[Number(index)]))
        }
      />
      {outcomes.length > 1 ? (
        <SelectMenu
          ariaLabel={t('factory.executorResultOutcome')}
          value={outcome?.key ?? ''}
          options={[
            { value: '', label: t('factory.chooseResultOutcome') },
            ...outcomes.map((item) => ({ value: item.key, label: item.key })),
          ]}
          onValueChange={setOutcomeKey}
        />
      ) : null}
      {selected && !command ? <p role="alert">{t('factory.staleExecutorChoice')}</p> : null}
      {!outcomes.length ? <p role="alert">{t('factory.executorNeedsOutcome')}</p> : null}
      {command ? (
        <p>
          {command.execution.model_or_tool.identity} · {command.execution.model_or_tool.version}
        </p>
      ) : null}
      {node.build ? <p>{t('factory.commandConfigured')}</p> : null}
      <button
        type="button"
        disabled={!command || !outcome}
        onClick={() => {
          if (command && outcome)
            onChange({
              build: { ...command.execution, result_outcome: outcome.key },
              profile_selection: null,
            });
        }}
      >
        {t('factory.useCommand')}
      </button>
    </fieldset>
  );
}
