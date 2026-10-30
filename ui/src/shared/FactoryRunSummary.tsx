import { Link } from 'react-router-dom';
import { usePresentation } from '../app/presentation/PresentationProvider';
import type { components } from '../../.generated/api/schema';
type FactoryRun = Pick<
  components['schemas']['FactoryRunSummaryResource'],
  'diagnostics' | 'state' | 'version'
> & { issue_association?: components['schemas']['IssueWorkAssociation'] | null };
import styles from './FactoryRead.module.css';

const metrics = [
  ['attempts', 'max_attempts', 'factory.budgetAttempts'],
  ['tokens', 'max_tokens', 'factory.budgetTokens'],
  ['cost_micro_units', 'max_cost_micro_units', 'factory.budgetCost'],
  ['elapsed_millis', 'max_elapsed_millis', 'factory.budgetTime'],
  ['output_bytes', 'max_output_bytes', 'factory.budgetOutput'],
] as const;

/** Summarises recorded facts without reconstructing decisions or execution policy in the browser. */
export function FactoryRunSummary({ run }: { run: FactoryRun }) {
  const { t } = usePresentation();
  const { diagnostics } = run;
  const association = run.issue_association;
  const selection = association
    ? new URLSearchParams({
        section: 'runs',
        ...(association.request_id
          ? { execution: association.request_id }
          : { work: association.work_id }),
      })
    : null;
  return (
    <section className={styles.panel} aria-label={t('factory.runOverview')}>
      <h2>{t('factory.runOverview')}</h2>
      <p className={styles.supportingText}>{t('factory.authoritativeRun')}</p>
      {association ? (
        <Link
          to={`/projects/${encodeURIComponent(association.project_id)}/issues/${encodeURIComponent(association.issue_id)}?${selection}`}
        >
          {t('factoryActions.task')}
        </Link>
      ) : run.issue_association === null ? (
        <p>{t('issues.taskLinkUnavailable')}</p>
      ) : null}
      <dl className={styles.runFacts}>
        <div>
          <dt>{t('factory.state')}</dt>
          <dd>
            {run.state} · v{run.version}
          </dd>
        </div>
        <div>
          <dt>{t('factory.currentActiveAttempts')}</dt>
          <dd>{diagnostics.active_node_count}</dd>
        </div>
        <div>
          <dt>{t('factory.baseRevision')}</dt>
          <dd className={styles.identity}>{diagnostics.base_revision}</dd>
        </div>
        <div>
          <dt>{t('factory.candidate')}</dt>
          <dd className={styles.identity}>
            {diagnostics.candidate?.candidate_revision ?? t('factory.noCandidate')}
          </dd>
        </div>
        <div>
          <dt>{t('factory.decision')}</dt>
          <dd>{diagnostics.latest_decision?.outcome ?? t('factory.noDecision')}</dd>
        </div>
        <div>
          <dt>{t('factory.delivery')}</dt>
          <dd>{diagnostics.delivery?.state ?? t('factory.noDelivery')}</dd>
        </div>
      </dl>
      <h3>{t('factory.diag.budgets')}</h3>
      <p className={styles.supportingText}>{t('factory.budgetUsageLimit')}</p>
      <dl className={styles.budgetMetrics}>
        {metrics.map(([used, limit, label]) => {
          const amount = diagnostics.budget.usage[used];
          return (
            <div key={used}>
              <dt>{t(label)}</dt>
              <dd>
                {typeof amount === 'number' ? (
                  <>
                    {amount} <span>/ {diagnostics.budget.limit[limit]}</span>
                  </>
                ) : (
                  <>
                    <span>{t('factory.costUnknown')}</span>
                    {amount.known_micro_units > 0 ? (
                      <span>
                        {' '}
                        ·{' '}
                        {t('factory.knownCostSubtotal', {
                          used: amount.known_micro_units,
                          limit: diagnostics.budget.limit[limit],
                        })}
                      </span>
                    ) : null}
                  </>
                )}
              </dd>
            </div>
          );
        })}
      </dl>
    </section>
  );
}
