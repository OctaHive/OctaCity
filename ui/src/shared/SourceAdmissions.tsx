import { Link } from 'react-router-dom';
import type { components } from '../../.generated/api/schema';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { formatTimestamp } from './display';

/** Native identity, immutable Work/Run and optional authoritative card association stay separate. */
export function SourceAdmissions({
  admissions,
}: {
  admissions: components['schemas']['WorkSourceAdmissionDiagnostic'][];
}) {
  const { t, locale } = usePresentation();
  return (
    <ul>
      {admissions.map((admission) => {
        const { association, reporting } = admission;
        const run = `/factory/${encodeURIComponent(association.run_id)}`;
        return (
          <li key={association.id}>
            <p>
              {t('factory.nativeCorrelation')}: {association.native_correlation}
            </p>
            <p>
              {t('factory.admittedVersion', {
                id: association.input_id,
                version: association.input_version,
              })}
            </p>
            <Link to={run}>{t('integrations.openRun')}</Link>{' '}
            <Link
              to={`${run}?diagnostic=work&diagnostic_id=${encodeURIComponent(association.work_id)}`}
            >
              {t('factory.diag.work')}
            </Link>
            {admission.issue ? (
              <>
                {' '}
                <Link to={`/issues/${encodeURIComponent(admission.issue.issue_id)}`}>
                  {t('factory.openTaskCard')}
                </Link>
              </>
            ) : null}
            <ul>
              {reporting.map((operation) => (
                <li key={operation.operation_id}>
                  <span>{t(`factory.projection.${operation.state}`)}</span> ·{' '}
                  {operation.operation_id}
                  {operation.origin ? (
                    <>
                      <p>
                        <Link
                          to={`${run}?diagnostic=node-attempts&diagnostic_id=${encodeURIComponent(operation.origin.action.node_attempt_id)}`}
                        >
                          {operation.origin.action.node} · {operation.origin.action.outcome}
                        </Link>{' '}
                        · {operation.origin.action.definition.id} · v
                        {operation.origin.action.definition.version} ·{' '}
                        {operation.origin.action.flow_run_id}
                      </p>
                      <ul>
                        {operation.origin.stages.map((stage) => (
                          <li key={stage.node_attempt_id}>
                            <Link
                              to={`${run}?diagnostic=node-attempts&diagnostic_id=${encodeURIComponent(stage.node_attempt_id)}`}
                            >
                              {stage.node} · {stage.outcome}
                            </Link>{' '}
                            · {stage.definition.id} · v{stage.definition.version} ·{' '}
                            {stage.flow_run_id}
                          </li>
                        ))}
                      </ul>
                    </>
                  ) : null}
                  <p>{t('factory.projectionAttempt', { attempt: operation.attempt })}</p>
                  {operation.state === 'pending' || operation.state === 'claimed' ? (
                    <dl>
                      <dt>{t('factory.projectionAvailable')}</dt>
                      <dd>{formatTimestamp(operation.available_at, locale).display}</dd>
                    </dl>
                  ) : null}
                </li>
              ))}
            </ul>
            {admission.reporting_truncated ? <p>{t('integrations.reportingTruncated')}</p> : null}
          </li>
        );
      })}
    </ul>
  );
}
