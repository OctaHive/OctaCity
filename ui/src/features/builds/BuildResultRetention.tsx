import { useQuery } from '@tanstack/react-query';

import { queryKeys } from '../../app/query';
import { formatTimestamp } from '../../shared/display';
import type { BuildResultDiagnosticsApi, BuildResultRetentionResource } from './api';
import { DiagnosticFailure, DiagnosticLoading, DiagnosticStale } from './BuildDiagnosticState';
import styles from './Builds.module.css';

export function BuildResultRetention({
  api,
  buildId,
}: {
  api: BuildResultDiagnosticsApi;
  buildId: string;
}) {
  const retention = useQuery({
    queryFn: ({ signal }) => api.getBuildResultRetention(buildId, signal),
    queryKey: queryKeys.buildResultRetention(buildId),
  });
  return (
    <section aria-label="Build Result retention" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Lifecycle evidence</p>
          <h2>Build Result retention</h2>
        </div>
        {retention.data === undefined ? null : <HoldStatus retention={retention.data} />}
      </div>
      {retention.isPending ? (
        <DiagnosticLoading label="Build Result retention" />
      ) : retention.data === undefined ? (
        <DiagnosticFailure
          error={retention.error}
          label="Build Result retention"
          onRetry={retention.refetch}
        />
      ) : (
        <RetentionDetails retention={retention.data} />
      )}
      {retention.data === undefined || retention.error === null ? null : (
        <DiagnosticStale label="Build Result retention" onRetry={retention.refetch} />
      )}
    </section>
  );
}

function HoldStatus({ retention }: { retention: BuildResultRetentionResource }) {
  const state = retention.hold?.state ?? 'absent';
  const label =
    state === 'absent'
      ? 'No hold'
      : state === 'active'
        ? 'Active hold'
        : state === 'expired'
          ? 'Expired hold'
          : 'Released hold';
  const family = state === 'active' ? 'info' : state === 'expired' ? 'danger' : 'muted';
  return <span className={`${styles.status} ${styles[`state_${family}`]}`}>{label}</span>;
}

function RetentionDetails({ retention }: { retention: BuildResultRetentionResource }) {
  const hold = retention.hold;
  return (
    <div className={styles.retentionDetails}>
      {hold === null ? null : (
        <dl className={styles.holdDetails}>
          <DefinitionTerm label="Reason" value={hold.reason} />
          <DefinitionTerm
            label="Hold expiry"
            value={
              hold.expires_at_unix_ms === null
                ? 'Permanent'
                : formatTimestamp(hold.expires_at_unix_ms).display
            }
          />
          <DefinitionTerm label="Version" value={String(hold.version)} />
          <DefinitionTerm label="Placement request" value={hold.creation_audit.request_identity} />
        </dl>
      )}
      <dl className={styles.retentionGrid}>
        <RetentionComponent
          deadline={retention.deadlines.metadata_at_unix_ms}
          label="Metadata"
          visible={retention.visibility.metadata}
        />
        <RetentionComponent
          deadline={retention.deadlines.logs_at_unix_ms}
          label="Logs"
          visible={retention.visibility.logs}
        />
        <RetentionComponent
          deadline={retention.deadlines.artifacts_at_unix_ms}
          label="Artifacts"
          visible={retention.visibility.artifacts}
        />
        <RetentionComponent
          deadline={retention.deadlines.reports_at_unix_ms}
          label="Reports"
          visible={retention.visibility.reports}
        />
      </dl>
    </div>
  );
}

function RetentionComponent({
  deadline,
  label,
  visible,
}: {
  deadline: number;
  label: string;
  visible: boolean;
}) {
  const timestamp = formatTimestamp(deadline);
  return (
    <div>
      <dt>{label}</dt>
      <dd>{visible ? 'Visible' : 'Unavailable'}</dd>
      <dd className={styles.secondaryMetadata}>
        Deadline <time dateTime={timestamp.machine ?? undefined}>{timestamp.display}</time>
      </dd>
    </div>
  );
}

function DefinitionTerm({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
