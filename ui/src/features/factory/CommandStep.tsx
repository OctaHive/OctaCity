import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Command = Schema['FactoryAuthoringCapabilities']['commands'][number];

function commandChoiceLabel({ alias, execution }: Command) {
  const { build_configuration, result_output, tool } = execution;
  return `${alias} · v${build_configuration.version} · ${result_output} · ${tool.identity}`;
}

/** Keep commands with identical display names individually selectable. */
export function commandOptions(commands: readonly Command[]) {
  const labels = commands.map(commandChoiceLabel);
  return commands.map((command, index) => {
    const label = labels[index]!;
    return {
      value: JSON.stringify(command),
      label:
        labels.indexOf(label) === labels.lastIndexOf(label)
          ? label
          : `${index + 1}. ${command.execution.task_digest.slice(0, 8)} · ${label}`,
    };
  });
}

/** Retains the installed result and evidence contract when binding a logical outcome. */
export function commandBuild(command: Command, outcome: string): Schema['FactoryFlowBuildBinding'] {
  return {
    ...command.execution,
    result_outcome: outcome,
    ...(command.result_tool ? { result_tool: command.result_tool } : {}),
    ...(command.evidence?.length ? { evidence: command.evidence } : {}),
  };
}

/** Chooses a declared ordinary compiler; command arguments and task identity remain installation-owned. */
export function CommandStep({
  node,
  commands,
  onChange,
}: {
  node: Node;
  commands: Schema['FactoryAuthoringCapabilities']['commands'];
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState('');
  const [outcomeKey, setOutcomeKey] = useState('');
  const command = commands.find((item) => JSON.stringify(item) === selected);
  const definition = command?.output_definition;
  const outcomes = node.outcomes.filter(
    (item) =>
      item.kind === 'success' &&
      (definition ||
        !command?.output_schema ||
        sourceReferenceKey(command.output_schema) === sourceReferenceKey(item.schema)),
  );
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
        value={command ? selected : ''}
        options={[{ value: '', label: t('factory.chooseInput') }, ...commandOptions(commands)]}
        onValueChange={setSelected}
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
          if (command && outcome) {
            onChange(
              {
                build: commandBuild(command, outcome.key),
                profile_selection: null,
                outcomes: node.outcomes.map((item) =>
                  item === outcome &&
                  definition &&
                  item.schema.digest !== definition.reference.digest
                    ? { ...item, schema: definition.reference }
                    : item,
                ),
              },
              definition ? [definition] : undefined,
            );
          }
        }}
      >
        {t('factory.useCommand')}
      </button>
    </fieldset>
  );
}
