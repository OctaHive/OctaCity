import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import styles from './Factory.module.css';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { BUDGET_LABELS } from './ResourceEditors';
type S = components['schemas'];
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
  configuration_wip: 'factory.limitSource.configurationWip',
  project_policy: 'factory.limitSource.project',
  installed_runtime_policy: 'factory.limitSource.runtime',
  agent_local: 'factory.limitSource.agent',
  backend_placement: 'factory.limitSource.backend',
} satisfies Record<
  | S['FactoryBudgetConstraintSource']['source']
  | S['FactoryConcurrencyConstraint']['source']
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
          <p>
            {t('factory.permissionCounts', {
              plugins: item.permissions.plugins.length,
              tools: item.permissions.tools.length,
              hosts: item.permissions.network_hosts.length,
            })}
          </p>
          <p>
            {t('factory.constraintOrigins')}:{' '}
            {(item.sources ?? []).map((source) => t(constraintSourceLabels[source])).join(', ')}
          </p>
          {item.unresolved_sources?.length ? (
            <p role="alert">
              {t('factory.unresolvedAccessOrigins', {
                sources: item.unresolved_sources
                  .map((source) => t(constraintSourceLabels[source]))
                  .join(', '),
              })}
            </p>
          ) : null}
          <details>
            <summary>{t('factory.routeDetails')}</summary>
            <dl>
              {Object.entries(item.permissions.resources).map(([key, value]) => (
                <div key={key}>
                  <dt>{key}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
          </details>
        </section>
      ))}
    </section>
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
