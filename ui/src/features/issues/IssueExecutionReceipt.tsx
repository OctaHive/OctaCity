import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueExecution } from './executionApi';
/** Preparation and admission describe publication only; Run outcome and business status are separate. */
export function ExecutionReceipt({
  execution,
  currentVersion,
}: {
  execution: IssueExecution;
  currentVersion: number;
}) {
  const { t } = usePresentation();
  return (
    <>
      <p>
        {t(execution.state === 'pending' ? 'issues.executionPending' : 'issues.executionAdmitted')}
      </p>
      <p>{t('issues.executionReview', { version: execution.intent.expected_version })}</p>
      {execution.intent.expected_version !== currentVersion ? (
        <p>{t('issues.executionEarlierRequirements')}</p>
      ) : null}
      {execution.association ? (
        <>
          <p>
            {t('issues.executionAssociation', {
              work: execution.association.work_id,
              run: execution.association.run_id,
            })}
          </p>
          <Link to={`/factory/${encodeURIComponent(execution.association.run_id)}`}>
            {t('issues.openRun')}
          </Link>
        </>
      ) : null}
    </>
  );
}
