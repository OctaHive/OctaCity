import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { BudgetEditor } from './ResourceEditors';

type Definition = components['schemas']['FactoryConfigurationDefinition'];
type Choice = components['schemas']['FactoryConfigurationChoiceResource'];
/** Aliases are selected from Project-scoped server choices and resolved by publication later. */
export function ConfigurationEditor({
  value,
  choices,
  flows,
  onChange,
}: {
  value: Definition;
  choices: readonly Choice[];
  flows: readonly components['schemas']['FactoryFlowDefinitionDetails'][];
  onChange: (value: Definition) => void;
}) {
  const { t } = usePresentation();
  const policyPicker = (kind: 'admission_policy' | 'permission_ceiling') => (
    <SelectMenu
      ariaLabel={t(
        kind === 'admission_policy' ? 'factory.admissionPolicy' : 'factory.permissionCeiling',
      )}
      value={value[kind]}
      options={[
        { value: '', label: t('factory.none') },
        ...choices
          .filter((choice) => choice.kind === kind && choice.reference.kind === 'immutable')
          .map((choice) => ({
            value: choice.alias,
            label: `${choice.alias} · ${choice.reference.reference.version}`,
          })),
      ]}
      onValueChange={(alias) => onChange({ ...value, [kind]: alias })}
    />
  );
  return (
    <fieldset data-factory-field="" tabIndex={-1}>
      <legend>{t('factory.configurationDraft')}</legend>
      <fieldset>
        <legend>{t('factory.startAndQueue')}</legend>
        <p>{t('factory.configurationQueueHelp')}</p>
        <label>
          <input
            type="checkbox"
            checked={value.enabled}
            onChange={(event) => onChange({ ...value, enabled: event.target.checked })}
          />
          {t('factory.enabled')}
        </label>
        {policyPicker('admission_policy')}
        <SelectMenu
          ariaLabel={t('factory.rootFlow')}
          value={String(
            flows.findIndex(
              (flow) =>
                flow.flow.closure.root.id === value.flow.closure.root.id &&
                flow.flow.closure.root.version === value.flow.closure.root.version,
            ),
          )}
          options={flows.map((flow, index) => ({
            value: String(index),
            label: `${flow.flow.closure.definitions.find((definition) => definition.reference.id === flow.flow.closure.root.id)?.entry ?? t('factory.immutableFlow')} · v${flow.flow.closure.root.version}`,
          }))}
          onValueChange={(index) => {
            const flow = flows[Number(index)];
            if (flow) onChange({ ...value, flow: structuredClone(flow.flow) });
          }}
        />
        {(['max_active_runs', 'max_active_nodes'] as const).map((key) => (
          <label key={key}>
            {t(key === 'max_active_runs' ? 'factory.activeRuns' : 'factory.activeCalls')}
            <input
              type="number"
              min={1}
              max={Number.MAX_SAFE_INTEGER}
              value={value.wip_limits[key]}
              onChange={(event) =>
                onChange({
                  ...value,
                  wip_limits: { ...value.wip_limits, [key]: Number(event.target.value) },
                })
              }
            />
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t('factory.executionLimits')}</legend>
        <p>{t('factory.configurationBudgetHelp')}</p>
        <BudgetEditor
          fieldPath="/hard_budget"
          label={t('factory.hardBudget')}
          value={value.hard_budget}
          onChange={(hard_budget) => onChange({ ...value, hard_budget })}
        />
      </fieldset>
      <fieldset>
        <legend>{t('factory.pipelineAccess')}</legend>
        <p>{t('factory.configurationAccessHelp')}</p>
        {policyPicker('permission_ceiling')}
      </fieldset>
    </fieldset>
  );
}
