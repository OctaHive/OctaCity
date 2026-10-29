import type { components } from '../../../.generated/api/schema';
import { FACTORY_SETUP_SCHEMAS } from '../../../.generated/api/factory-setup';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';
import { emptyFlowDraft } from './emptyFlowDraft';

type Schema = components['schemas'];
export type ReportingPolicy = Pick<
  Schema['FactoryResultReportingDraft'],
  'budget' | 'execution' | 'retry'
>;
const retry = FACTORY_SETUP_SCHEMAS.FactoryResultReportingDraft.properties.retry.properties;
/** Initial values are bounded drafts; they grant no execution permissions or publication readiness. */
export function initialReportingPolicy(): ReportingPolicy {
  const execution = emptyFlowDraft().root.execution;
  return {
    budget: { ...execution.budget },
    execution,
    retry: {
      max_attempts: retry.max_attempts.minimum,
      retry_delay_millis: retry.retry_delay_millis.minimum,
    },
  };
}
/** Operator policy is reviewed independently of the installed producer's immutable receipt. */
export function TriageReportingPolicy({
  value,
  ceiling,
  onChange,
}: {
  value: ReportingPolicy;
  ceiling: Schema['FactoryFlowExecutionPolicy'];
  onChange: (value: ReportingPolicy) => void;
}) {
  const { t } = usePresentation();
  return (
    <fieldset>
      <legend>{t('triage.reportingPolicy')}</legend>
      <BudgetEditor
        value={value.budget}
        label={t('triage.reportingNodeBudget')}
        ceiling={ceiling.budget}
        onChange={(budget) => onChange({ ...value, budget })}
      />
      <BudgetEditor
        value={value.execution.budget}
        label={t('triage.reportingFlowBudget')}
        ceiling={ceiling.budget}
        onChange={(budget) => onChange({ ...value, execution: { ...value.execution, budget } })}
      />
      <PermissionEditor
        value={value.execution.permissions}
        choices={ceiling.permissions}
        ceiling={ceiling.permissions}
        label={t('triage.reportingPermissions')}
        onChange={(permissions) =>
          onChange({ ...value, execution: { ...value.execution, permissions } })
        }
      />
      <label>
        {t('triage.reportingConcurrency')}
        <input
          type="number"
          min={1}
          max={ceiling.max_active_nodes}
          value={value.execution.max_active_nodes}
          onChange={(event) =>
            onChange({
              ...value,
              execution: { ...value.execution, max_active_nodes: Number(event.target.value) },
            })
          }
        />
      </label>
      <label>
        {t('triage.reporterAttempts')}
        <input
          type="number"
          min={retry.max_attempts.minimum}
          max={retry.max_attempts.maximum}
          value={value.retry.max_attempts}
          onChange={(event) =>
            onChange({
              ...value,
              retry: { ...value.retry, max_attempts: Number(event.target.value) },
            })
          }
        />
      </label>
      <label>
        {t('triage.reporterRetryDelay')}
        <input
          type="number"
          min={retry.retry_delay_millis.minimum}
          max={retry.retry_delay_millis.maximum}
          value={value.retry.retry_delay_millis}
          onChange={(event) =>
            onChange({
              ...value,
              retry: { ...value.retry, retry_delay_millis: Number(event.target.value) },
            })
          }
        />
      </label>
    </fieldset>
  );
}
