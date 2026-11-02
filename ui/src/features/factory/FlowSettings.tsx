import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { PermissionEditor } from './ResourceEditors';
import { BudgetSettings } from './BudgetSettings';
import { flowBudget } from './flowRequirements';
type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
const LIMITS = {
  max_depth: 'factory.depth',
  max_expanded_nodes: 'factory.expandedNodes',
  max_fan_out: 'factory.fanOut',
  max_repeat_count: 'factory.repeatBound',
  max_wip: 'factory.wip',
} satisfies Record<Exclude<keyof Draft['limits'], 'budget' | 'permissions'>, MessageKey>;
export function FlowSettings({
  value,
  choices,
  permissionDeclarations,
  onChange,
  budgetCalculation,
  onBudgetCalculationChange,
  automaticBounds = false,
  onAutomaticBoundsChange,
}: {
  value: Draft;
  choices: readonly components['schemas']['FactoryConfigurationChoiceResource'][];
  permissionDeclarations: components['schemas']['FactoryPermissions'];
  onChange: (draft: Draft) => void;
  budgetCalculation: { run: boolean; admission: boolean };
  onBudgetCalculationChange: (kind: 'run' | 'admission', automatic: boolean) => void;
  automaticBounds?: boolean;
  onAutomaticBoundsChange?: (automatic: boolean) => void;
}) {
  const { t } = usePresentation();
  const terminalKeys = useDraftRowKeys(value.root.terminals);
  return (
    <fieldset data-factory-field="/limits" tabIndex={-1}>
      <legend>{t('factory.flowSettings')}</legend>
      <fieldset>
        <legend>{t('factory.startAndQueue')}</legend>
        <p>{t('factory.runQueueHelp')}</p>
        <SelectMenu
          ariaLabel={t('factory.entry')}
          value={value.root.entry}
          options={value.root.nodes.map((node) => ({ value: node.key, label: node.key }))}
          onValueChange={(entry) => onChange({ ...value, root: { ...value.root, entry } })}
        />
        <details>
          <summary>{t('factory.advancedContracts')}</summary>
          <SelectMenu
            ariaLabel={t('factory.flowInputSchema')}
            value={String(
              value.data_schemas.findIndex(
                (schema) => schema.reference.digest === value.root.input_schema?.digest,
              ),
            )}
            options={[
              { value: '-1', label: t('factory.none') },
              ...value.data_schemas.map((schema, index) => ({
                value: String(index),
                label: `${schema.reference.identity} · ${schema.reference.version}`,
              })),
            ]}
            onValueChange={(index) =>
              onChange({
                ...value,
                root: {
                  ...value.root,
                  input_schema: value.data_schemas[Number(index)]?.reference ?? null,
                },
              })
            }
          />
        </details>
        <label>
          {t('factory.activeNodes')}
          <input
            type="number"
            min={1}
            max={value.limits.max_wip}
            value={value.root.execution.max_active_nodes}
            onChange={(event) =>
              onChange({
                ...value,
                root: {
                  ...value.root,
                  execution: {
                    ...value.root.execution,
                    max_active_nodes: Number(event.target.value),
                  },
                },
              })
            }
          />
        </label>
        <label>
          {t('factory.runNodeCeiling')}
          <input
            type="number"
            min={1}
            max={Number.MAX_SAFE_INTEGER}
            disabled={automaticBounds}
            value={value.limits.max_wip}
            onChange={(event) =>
              onChange({
                ...value,
                limits: { ...value.limits, max_wip: Number(event.target.value) },
              })
            }
          />
        </label>
      </fieldset>
      <fieldset>
        <legend>{t('factory.executionLimits')}</legend>
        <p>{t('factory.runBudgetHelp')}</p>
        <BudgetSettings
          automatic={budgetCalculation.run}
          onAutomaticChange={(automatic) => onBudgetCalculationChange('run', automatic)}
          suggested={flowBudget(value.root)}
          source={t('factory.flowBudgetSource')}
          fieldPath="/root/execution/budget"
          label={t('factory.executionBudget')}
          value={value.root.execution.budget}
          ceiling={value.limits.budget}
          onChange={(budget) =>
            onChange({
              ...value,
              root: { ...value.root, execution: { ...value.root.execution, budget } },
            })
          }
        />
        <details>
          <summary>{t('factory.admissionLimitsDetails')}</summary>
          <p>{t('factory.admissionCeilingHelp')}</p>
          {onAutomaticBoundsChange ? (
            <SelectMenu
              ariaLabel={t('factory.graphBoundsCalculation')}
              value={automaticBounds ? 'automatic' : 'manual'}
              options={[
                { value: 'automatic', label: t('factory.calculateAutomatically') },
                { value: 'manual', label: t('factory.setManually') },
              ]}
              onValueChange={(mode) => {
                onAutomaticBoundsChange(mode === 'automatic');
              }}
            />
          ) : null}
          {(Object.keys(LIMITS) as (keyof typeof LIMITS)[])
            .filter((key) => key !== 'max_wip')
            .map((key) => (
              <label key={key}>
                {t('factory.admission')} · {t(LIMITS[key])}
                <input
                  type="number"
                  min={0}
                  max={Number.MAX_SAFE_INTEGER}
                  disabled={automaticBounds}
                  value={value.limits[key]}
                  onChange={(event) =>
                    onChange({
                      ...value,
                      limits: { ...value.limits, [key]: Number(event.target.value) },
                    })
                  }
                />
              </label>
            ))}
          <BudgetSettings
            automatic={budgetCalculation.admission}
            onAutomaticChange={(automatic) => onBudgetCalculationChange('admission', automatic)}
            suggested={value.root.execution.budget}
            source={t('factory.admissionBudgetSource')}
            fieldPath="/limits/budget"
            label={t('factory.admissionBudget')}
            value={value.limits.budget}
            onChange={(budget) => onChange({ ...value, limits: { ...value.limits, budget } })}
          />
        </details>
      </fieldset>
      <fieldset>
        <legend>{t('factory.pipelineAccess')}</legend>
        <p>{t('factory.pipelineAccessHelp')}</p>
        <details>
          <summary>{t('factory.manualPipelineAccess')}</summary>
          <PermissionEditor
            fieldPath="/root/execution/permissions"
            label={t('factory.executionPermissions')}
            value={value.root.execution.permissions}
            ceiling={value.limits.permissions}
            choices={choices}
            onChange={(permissions) =>
              onChange({
                ...value,
                root: { ...value.root, execution: { ...value.root.execution, permissions } },
              })
            }
          />
          <details>
            <summary>{t('factory.admissionAccessDetails')}</summary>
            <PermissionEditor
              fieldPath="/limits/permissions"
              label={t('factory.admissionPermissions')}
              value={value.limits.permissions}
              ceiling={permissionDeclarations}
              choices={choices}
              onChange={(permissions) =>
                onChange({ ...value, limits: { ...value.limits, permissions } })
              }
            />
          </details>
        </details>
      </fieldset>
      <h4>{t('factory.terminals')}</h4>
      <button
        type="button"
        disabled={value.data_schemas.length === 0}
        onClick={() => {
          const schema = value.data_schemas[0];
          if (schema)
            onChange({
              ...value,
              root: {
                ...value.root,
                terminals: [...value.root.terminals, { key: '', schema: schema.reference }],
              },
            });
        }}
      >
        {t('factory.addTerminal')}
      </button>
      {value.root.terminals.map((terminal, index) => (
        <fieldset key={terminalKeys[index]}>
          <label>
            {t('factory.terminalName')}
            <input
              maxLength={128}
              value={terminal.key}
              onChange={(event) => {
                const key = event.target.value;
                onChange({
                  ...value,
                  root: {
                    ...value.root,
                    terminals: value.root.terminals.map((item, position) =>
                      position === index ? { ...item, key } : item,
                    ),
                    transitions: value.root.transitions.map((edge) => ({
                      ...edge,
                      target:
                        edge.target.kind === 'terminal' && edge.target.key === terminal.key
                          ? { kind: 'terminal', key }
                          : edge.target,
                      exhausted_terminal:
                        edge.exhausted_terminal === terminal.key ? key : edge.exhausted_terminal,
                    })),
                  },
                });
              }}
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.terminalSchema')}
            value={String(
              value.data_schemas.findIndex(
                (schema) => schema.reference.digest === terminal.schema.digest,
              ),
            )}
            options={value.data_schemas.map((schema, index) => ({
              value: String(index),
              label:
                value.root.nodes
                  .flatMap((node) =>
                    node.outcomes
                      .filter((outcome) => outcome.schema.digest === schema.reference.digest)
                      .map((outcome) => `${node.key} › ${outcome.key}`),
                  )
                  .join(', ') || `${schema.reference.identity} · ${schema.reference.version}`,
            }))}
            onValueChange={(selected) => {
              const schema = value.data_schemas[Number(selected)];
              if (schema)
                onChange({
                  ...value,
                  root: {
                    ...value.root,
                    terminals: value.root.terminals.map((item, position) =>
                      position === index ? { ...item, schema: schema.reference } : item,
                    ),
                  },
                });
            }}
          />
        </fieldset>
      ))}
    </fieldset>
  );
}
