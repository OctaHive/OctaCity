import { useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { aiSettingsPath } from '../../app/routes';
import { useDeadline } from '../../shared/useDeadline';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { FactoryApi } from './api';

type Schema = components['schemas'];
const referenceKey = (value: Schema['FactoryConfigurationReference']) => [
  value.project_id,
  value.id,
  value.version,
  value.definition_digest,
];
const subjectKey = (
  configuration: Schema['FactoryConfigurationReference'],
  repository: Schema['FactoryReadinessRepository'] | undefined,
) => JSON.stringify([referenceKey(configuration), repository ? referenceKey(repository) : null]);
const reasons: Record<string, MessageKey> = {
  capacity_absent: 'triage.agentMissing',
  inventory_unavailable: 'triage.agentMissing',
  capture_unqualified: 'triage.captureMissing',
  credential_unverified: 'triage.authUnknown',
  configuration_unavailable: 'triage.installationMissing',
  transport_unsupported: 'triage.mcpMissing',
  scoped_authentication_unobserved: 'triage.octacityAuthUnknown',
  peer_scope_unverified: 'triage.scopeUnknown',
};
/** Exact metadata observations keep execution and native-write status separate; checking starts no work. */
export function TriageReadiness({
  api,
  configuration,
  repository,
}: {
  api: FactoryApi;
  configuration: Schema['FactoryConfigurationReference'];
  repository: Schema['FactoryReadinessRepository'] | undefined;
}) {
  const { t } = usePresentation();
  const [report, setReport] = useState<{
    subject: string;
    value: Schema['FactoryReadinessReport'];
  } | null>(null);
  const subject = subjectKey(configuration, repository);
  const latestSubjectRef = useRef(subject);
  latestSubjectRef.current = subject;
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  async function check() {
    if (!repository) return;
    const captured = subject;
    setPending(true);
    setError(null);
    try {
      const value = await api.readiness(configuration.id, configuration.version, { repository });
      if (
        latestSubjectRef.current === captured &&
        subjectKey(value.configuration, value.repository) === captured
      )
        setReport({ subject: captured, value });
    } catch (failure) {
      if (latestSubjectRef.current === captured)
        setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  return (
    <section aria-label={t('triage.readiness')}>
      <h3>{t('triage.readiness')}</h3>
      <p>{t('triage.authority')}</p>
      <button type="button" disabled={pending || !repository} onClick={() => void check()}>
        {t('triage.checkReadiness')}
      </button>
      {error ? (
        <QueryFailureNotice title={t('triage.readiness')} error={error} onRetry={check} />
      ) : null}
      {report?.subject === subject ? (
        <ReadinessObservation
          key={JSON.stringify([report.subject, report.value.observed_at])}
          report={report.value}
        />
      ) : null}
    </section>
  );
}

function ReadinessObservation({ report }: { report: Schema['FactoryReadinessReport'] }) {
  const { t } = usePresentation();
  const expired = useDeadline(report.expires_at);
  return (
    <>
      <p>
        {t(
          expired
            ? 'triage.observationExpired'
            : report.execution === 'ready'
              ? 'triage.ready'
              : report.execution === 'blocked'
                ? 'triage.blocked'
                : 'triage.unknown',
        )}
      </p>
      <p>
        {t(
          !expired && report.native_write === 'ready'
            ? 'triage.nativeWriteObserved'
            : 'triage.nativeWriteUnknown',
        )}
      </p>
      <ul>
        {report.checks
          .filter((item) => item.state !== 'ready')
          .map((item) => (
            <li key={JSON.stringify([item.aspect, item.subject, item.reason])}>
              {t(reasons[item.reason] ?? 'triage.dependencyUnknown')}{' '}
              <Link
                to={
                  item.remediation === 'enroll_compatible_agent'
                    ? '/agent-pools'
                    : item.remediation === 'review_source' || item.remediation === 'review_reporter'
                      ? '/integrations'
                      : aiSettingsPath(report.configuration.project_id, 'runtimes')
                }
                target="_blank"
                rel="noreferrer"
              >
                {t('triage.remediate')}
              </Link>
            </li>
          ))}
      </ul>
    </>
  );
}
