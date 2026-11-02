import { useState } from 'react';
import type { ReactNode } from 'react';
import type { components } from '../../../.generated/api/schema';
import { flowLabel } from './flowLabels';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { BudgetEditor } from './ResourceEditors';
import styles from './Factory.module.css';

export type ConfigurationSettings = Omit<
  components['schemas']['FactoryConfigurationDefinition'],
  'admission_policy' | 'permission_ceiling'
>;

/** Shared queue, graph and budget fields for new and replacement drafts. */
export function ConfigurationEditor({
  value,
  policies,
  flows,
  onChange,
}: {
  value: ConfigurationSettings;
  policies: { launch: ReactNode; access: ReactNode };
  flows: readonly components['schemas']['FactoryFlowDefinitionDetails'][];
  onChange: (value: ConfigurationSettings) => void;
}) {
  const { t } = usePresentation();
  const [originalFlow] = useState(value.flow);
  const options = new Map(
    [originalFlow, ...flows.map(({ flow }) => flow), value.flow].map((flow) => [
      flowVersionKey(flow),
      flow,
    ]),
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
        {policies.launch}
        <div className={styles.selectField}>
          <span>{t('factory.rootFlow')}</span>
          <SelectMenu
            ariaLabel={t('factory.rootFlow')}
            value={flowVersionKey(value.flow)}
            options={Array.from(options, ([value, flow]) => ({
              value,
              label: flowLabel(flow, t('factory.immutableFlow')),
            }))}
            onValueChange={(key) => {
              const flow = options.get(key);
              if (flow) onChange({ ...value, flow: structuredClone(flow) });
            }}
          />
          <p className={styles.supportingText}>{t('factory.rootFlowHelp')}</p>
        </div>
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
        {policies.access}
      </fieldset>
    </fieldset>
  );
}

function flowVersionKey(flow: ConfigurationSettings['flow']) {
  const { id, version, digest } = flow.closure.root;
  return `${id}:${version}:${digest}`;
}
