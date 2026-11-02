import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import styles from './Factory.module.css';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { BUDGET_LABELS } from './ResourceEditors';
type S = components['schemas'];
type NodePermissions = S['FactoryValidationDiagnostics']['effective_permissions'][number];
const permissionResourceLabels = {
  cpu_millis: 'factory.resourceCpu',
  elapsed_millis: 'factory.maxElapsed',
  memory_bytes: 'factory.resourceMemory',
  disk_bytes: 'factory.resourceDisk',
  process_count: 'factory.resourceProcesses',
} satisfies Record<keyof S['FactoryResourceLimits'], MessageKey>;
const scopes = {
  run_total: 'factory.scope.runTotal',
  flow_run_total: 'factory.scope.flowRunTotal',
  node_execution: 'factory.scope.nodeExecution',
  run_nodes: 'factory.scope.runNodes',
  flow_nodes: 'factory.scope.flowNodes',
  configuration_runs: 'factory.scope.configurationRuns',
  configuration_nodes: 'factory.scope.configurationNodes',
} satisfies Record<
  S['FactoryBudgetConstraint']['scope'] | S['FactoryConcurrencyConstraint']['scope'],
  MessageKey
>;
export const constraintSourceLabels = {
  flow_admission: 'factory.limitSource.flowAdmission',
  flow_definition: 'factory.limitSource.flowDefinition',
  node: 'factory.limitSource.node',
  configuration: 'factory.limitSource.configuration',
  role_profile: 'factory.limitSource.profile',
  profile_budget: 'factory.limitSource.profileBudget',
  configuration_wip: 'factory.limitSource.configurationWip',
  project_policy: 'factory.limitSource.project',
  installed_runtime_policy: 'factory.limitSource.runtime',
  installation: 'factory.limitSource.installation',
  agent_local: 'factory.limitSource.agent',
  backend_placement: 'factory.limitSource.backend',
} satisfies Record<
  | S['FactoryBudgetConstraintSource']['source']
  | S['FactoryConcurrencyConstraint']['source']
  | NonNullable<S['FactoryValidationDiagnostics']['errors'][number]['ceiling']>
  | NonNullable<NodePermissions['profiles']>[number]['sources'][number]
  | NonNullable<
      S['FactoryValidationDiagnostics']['effective_permissions'][number]['unresolved_sources']
    >[number],
  MessageKey
>;
/** Only renders the authoritative projection: no browser clamping or independently computed effective policy. */
export function ConstraintReview({
  diagnostics,
}: {
  diagnostics: S['FactoryValidationDiagnostics'];
}) {
  const { t, locale } = usePresentation();
  const constraints = diagnostics.constraints;
  const budgets = constraints ? [constraints.run_budget, ...constraints.node_budgets] : [];
  const budgetKeys = useDraftRowKeys(budgets);
  const concurrencyKeys = useDraftRowKeys(constraints?.concurrency ?? []);
  const format = (key: keyof S['FactoryBudget'], value: number) =>
    key === 'max_elapsed_millis'
      ? `${(value / 1000).toLocaleString(locale)} s`
      : key === 'max_output_bytes'
        ? `${value.toLocaleString(locale)} B`
        : key === 'max_cost_micro_units'
          ? `${(value / 1_000_000).toLocaleString(locale)} ${t('factory.costUnits')}`
          : value.toLocaleString(locale);
  return (
    <section role="region" aria-label={t('factory.effectiveConstraints')}>
      <h4>{t('factory.effectiveConstraints')}</h4>
      {!constraints ? (
        <p>{t('factory.constraintProjectionUnavailable')}</p>
      ) : (
        <>
          <div className={styles.constraintTable}>
            <table>
              <thead>
                <tr>
                  <th>{t('factory.constraintScope')}</th>
                  <th>{t('factory.constraintResource')}</th>
                  <th>{t('factory.requestedLimit')}</th>
                  <th>{t('factory.effectiveLimit')}</th>
                  <th>{t('factory.constraintOrigins')}</th>
                </tr>
              </thead>
              <tbody>
                {budgets.flatMap((item, position) =>
                  (Object.keys(BUDGET_LABELS) as (keyof S['FactoryBudget'])[]).map((key) => (
                    <tr key={`${budgetKeys[position]}:${key}`}>
                      <th scope="row">
                        {item.node ? `${item.node} · ` : ''}
                        {t(scopes[item.scope])}
                      </th>
                      <td>{t(BUDGET_LABELS[key])}</td>
                      <td>{format(key, item.requested[key])}</td>
                      <td>{format(key, item.effective[key])}</td>
                      <td>
                        <ConstraintOrigins item={item} resource={key} format={format} />
                      </td>
                    </tr>
                  )),
                )}
              </tbody>
            </table>
          </div>
          {constraints.run_budget.retry_charging === 'cumulative_in_run' ? (
            <p>{t('factory.retryCumulative')}</p>
          ) : null}
          <h5>{t('factory.concurrencyLimits')}</h5>
          <ul>
            {constraints.concurrency.map((item, index) => (
              <li key={concurrencyKeys[index]}>
                {t(scopes[item.scope])} · {t(constraintSourceLabels[item.source])} ·{' '}
                {t('factory.requestedLimit')}:{' '}
                {item.requested === null
                  ? t('factory.limitUnresolved')
                  : item.requested.toLocaleString(locale)}
                ; {t('factory.effectiveLimit')}:{' '}
                {item.effective === null
                  ? t('factory.limitUnresolved')
                  : item.effective.toLocaleString(locale)}
                {item.unresolved ? (
                  <p role="status">{t('factory.configurationRequiredForLimit')}</p>
                ) : null}
              </li>
            ))}
          </ul>
        </>
      )}
      <h4>{t('factory.effectivePermissions')}</h4>
      {diagnostics.effective_permissions.map((item) => (
        <section key={`${item.definition.id}:${item.node}`}>
          <h5>{item.node}</h5>
          <PermissionCeiling permissions={item.permissions} sources={item.sources ?? []} />
          {item.profiles?.length ? <p>{t('factory.profileCeilingsHelp')}</p> : null}
          {item.profiles?.map((candidate, index) => (
            <fieldset key={JSON.stringify(candidate.profile)}>
              <legend>
                {t('factory.profileCandidateCeiling', {
                  number: index + 1,
                  version: candidate.profile.version,
                })}
              </legend>
              <PermissionCeiling permissions={candidate.permissions} sources={candidate.sources} />
              <details>
                <summary>{t('factory.routeDetails')}</summary>
                <dl>
                  <dt>{t('factory.executorProfile')}</dt>
                  <dd>{candidate.profile.id}</dd>
                  <dt>{t('factory.digest')}</dt>
                  <dd>{candidate.profile.digest}</dd>
                </dl>
              </details>
            </fieldset>
          ))}
          {item.unresolved_sources?.length ? (
            <p role="alert">
              {t('factory.unresolvedAccessOrigins', {
                sources: item.unresolved_sources
                  .map((source) => t(constraintSourceLabels[source]))
                  .join(', '),
              })}
            </p>
          ) : null}
        </section>
      ))}
    </section>
  );
}

/** Displays independent server-projected ceilings; candidate policy is not a dispatch grant. */
function PermissionCeiling({
  permissions,
  sources,
}: {
  permissions: S['FactoryPermissions'];
  sources: readonly (keyof typeof constraintSourceLabels)[];
}) {
  const { t, locale } = usePresentation();
  return (
    <>
      <p>
        {t('factory.permissionCounts', {
          plugins: permissions.plugins.length,
          tools: permissions.tools.length,
          hosts: permissions.network_hosts.length,
        })}
      </p>
      <p>
        {t('factory.constraintOrigins')}:{' '}
        {sources.map((source) => t(constraintSourceLabels[source])).join(', ')}
      </p>
      <dl>
        {(Object.keys(permissionResourceLabels) as (keyof S['FactoryResourceLimits'])[]).map(
          (key) => (
            <div key={key}>
              <dt>{t(permissionResourceLabels[key])}</dt>
              <dd>{permissions.resources[key].toLocaleString(locale)}</dd>
            </div>
          ),
        )}
      </dl>
    </>
  );
}

function ConstraintOrigins({
  item,
  resource,
  format,
}: {
  item: S['FactoryBudgetConstraint'];
  resource: keyof S['FactoryBudget'];
  format: (key: keyof S['FactoryBudget'], value: number) => string;
}) {
  const { t } = usePresentation();
  const keys = useDraftRowKeys(item.sources);
  return item.sources.map((origin, index) => (
    <span key={keys[index]}>
      {t(constraintSourceLabels[origin.source])} · {t(scopes[origin.scope])} ·{' '}
      {format(resource, origin.limit[resource])}
      {index < item.sources.length - 1 ? '; ' : ''}
    </span>
  ));
}
