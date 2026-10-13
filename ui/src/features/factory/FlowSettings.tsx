import { useDraftRowKeys } from './useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';
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
}: {
  value: Draft;
  choices: readonly components['schemas']['FactoryConfigurationChoiceResource'][];
  permissionDeclarations: components['schemas']['FactoryPermissions'];
  onChange: (draft: Draft) => void;
}) {
  const { t } = usePresentation();
  const terminalKeys = useDraftRowKeys(value.root.terminals);
  return (
    <fieldset data-factory-field="/limits" tabIndex={-1}>
      <legend>{t('factory.flowSettings')}</legend>
      <SelectMenu
        ariaLabel={t('factory.entry')}
        value={value.root.entry}
        options={value.root.nodes.map((node) => ({ value: node.key, label: node.key }))}
        onValueChange={(entry) => onChange({ ...value, root: { ...value.root, entry } })}
      />
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
      {(Object.keys(LIMITS) as (keyof typeof LIMITS)[]).map((key) => (
        <label key={key}>
          {t('factory.admission')} · {t(LIMITS[key])}
          <input
            type="number"
            min={0}
            max={Number.MAX_SAFE_INTEGER}
            value={value.limits[key]}
            onChange={(event) =>
              onChange({ ...value, limits: { ...value.limits, [key]: Number(event.target.value) } })
            }
          />
        </label>
      ))}
      <BudgetEditor
        label={t('factory.admissionBudget')}
        value={value.limits.budget}
        onChange={(budget) => onChange({ ...value, limits: { ...value.limits, budget } })}
      />
      <PermissionEditor
        label={t('factory.admissionPermissions')}
        value={value.limits.permissions}
        ceiling={permissionDeclarations}
        choices={choices}
        onChange={(permissions) => onChange({ ...value, limits: { ...value.limits, permissions } })}
      />
      <BudgetEditor
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
      <PermissionEditor
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
              label: `${schema.reference.identity} · ${schema.reference.version}`,
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
