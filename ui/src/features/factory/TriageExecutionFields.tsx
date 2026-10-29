import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch } from '../../shared/PagedExplorerBranch';
import { SelectMenu } from '../../shared/SelectMenu';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';
import { catalogLabels } from './catalogLabels';
import { emptyFlowDraft } from './emptyFlowDraft';
import type { FactoryApi } from './api';
import { useFactoryChoices } from './useFactoryChoices';

type Schema = components['schemas'];
export type TriagePolicies = {
  execution: Schema['FactoryFlowExecutionPolicy'];
  configuration: Schema['FactoryPresetConfigurationParameters'];
};
/** Uses ordinary policy choices and editors. Root limits are explicit; no role name grants authority. */
export function TriageExecutionFields({
  api,
  projectId,
  value,
  onChange,
}: {
  api: FactoryApi;
  projectId: string;
  value: TriagePolicies | undefined;
  onChange: (value: TriagePolicies) => void;
}) {
  const { t } = usePresentation();
  const choices = useFactoryChoices(api, projectId);
  const items = choices.data?.pages.flatMap((page) => page.items) ?? [];
  const execution = value?.execution ?? emptyFlowDraft().root.execution;
  const configuration = value?.configuration ?? {
    admission_policy: '',
    permission_ceiling: '',
    hard_budget: execution.budget,
    wip_limits: {
      max_active_runs: execution.max_active_nodes,
      max_active_nodes: execution.max_active_nodes,
    },
  };
  const ceiling = items.find(
    (item) => item.kind === 'permission_ceiling' && item.alias === configuration.permission_ceiling,
  )?.metadata?.permission_declarations;
  return (
    <fieldset>
      <legend>{t('triage.execution')}</legend>
      <PagedExplorerBranch
        query={choices}
        empty={t('factory.noChoices')}
        labels={catalogLabels(t('factory.choices'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {(['admission_policy', 'permission_ceiling'] as const).map((kind) => (
        <label key={kind}>
          <span>
            {t(
              kind === 'admission_policy' ? 'factory.admissionPolicy' : 'factory.permissionCeiling',
            )}
          </span>
          <SelectMenu
            ariaLabel={t(
              kind === 'admission_policy' ? 'factory.admissionPolicy' : 'factory.permissionCeiling',
            )}
            value={configuration[kind]}
            options={[
              { value: '', label: t('factory.none') },
              ...items
                .filter((item) => item.kind === kind && item.reference.kind === 'immutable')
                .map((item) => ({
                  value: item.alias,
                  label: `${item.alias} · ${item.reference.kind === 'immutable' ? item.reference.reference.version : ''}`,
                })),
            ]}
            onValueChange={(alias) =>
              onChange({ execution, configuration: { ...configuration, [kind]: alias } })
            }
          />
        </label>
      ))}
      <BudgetEditor
        label={t('factory.executionBudget')}
        value={execution.budget}
        onChange={(budget) => onChange({ configuration, execution: { ...execution, budget } })}
      />
      <PermissionEditor
        label={t('factory.executionPermissions')}
        value={execution.permissions}
        choices={items}
        {...(ceiling ? { ceiling } : {})}
        onChange={(permissions) =>
          onChange({ configuration, execution: { ...execution, permissions } })
        }
      />
      <BudgetEditor
        label={t('factory.hardBudget')}
        value={configuration.hard_budget}
        onChange={(hard_budget) =>
          onChange({ execution, configuration: { ...configuration, hard_budget } })
        }
      />
      {(['max_active_runs', 'max_active_nodes'] as const).map((field) => (
        <label key={field}>
          {t(field === 'max_active_runs' ? 'factory.activeRuns' : 'factory.activeCalls')}
          <input
            type="number"
            min={1}
            max={Number.MAX_SAFE_INTEGER}
            value={configuration.wip_limits[field]}
            onChange={(event) =>
              onChange({
                execution,
                configuration: {
                  ...configuration,
                  wip_limits: { ...configuration.wip_limits, [field]: Number(event.target.value) },
                },
              })
            }
          />
        </label>
      ))}
      <label>
        {t('triage.rootConcurrency')}
        <input
          type="number"
          min={1}
          max={Number.MAX_SAFE_INTEGER}
          value={execution.max_active_nodes}
          onChange={(event) =>
            onChange({
              configuration,
              execution: { ...execution, max_active_nodes: Number(event.target.value) },
            })
          }
        />
      </label>
    </fieldset>
  );
}
