import type { components } from '../../../.generated/api/schema';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SafeFactoryMetadata } from '../../shared/FactoryDiagnosticRecord';
/** Exact producer identity is shared by check and artifact provenance, without object traversal. */
export function IssueResultProducer({
  producer,
}: {
  producer: components['schemas']['IssueEvidenceProducer'] | null;
}) {
  const { t } = usePresentation();
  return producer ? (
    <>
      <SafeFactoryMetadata
        value={producer}
        fields={['build_id', 'attempt_id', 'job_id', 'plugin', 'tool']}
      />
      <Link
        to={`/builds/${encodeURIComponent(producer.build_id)}?attempt_id=${encodeURIComponent(producer.attempt_id)}&job_id=${encodeURIComponent(producer.job_id)}`}
      >
        {t('issueResults.openProducer')}
      </Link>
    </>
  ) : null;
}
