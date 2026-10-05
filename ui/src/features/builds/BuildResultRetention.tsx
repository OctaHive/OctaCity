import { useQuery } from '@tanstack/react-query';
import { useRef, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { auditRequestPath } from '../../app/routes';
import { formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { BuildResultHoldApi, BuildResultRetentionResource } from './api';
import { BuildResultHoldCommands } from './BuildResultHoldCommands';
import styles from './Builds.module.css';

export function BuildResultRetention({
  api,
  buildId,
}: {
  api: BuildResultHoldApi;
  buildId: string;
}) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const retention = useQuery({
    queryFn: ({ signal }) => api.getBuildResultRetention(buildId, signal),
    queryKey: queryKeys.buildResultRetention(buildId),
  });
  return (
    <section aria-label="Build Result retention" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Lifecycle evidence</p>
          <h2 ref={headingRef} tabIndex={-1}>
            Build Result retention
          </h2>
        </div>
        {retention.data === undefined ? null : <HoldStatus retention={retention.data} />}
      </div>
      {retention.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label="Build Result retention" />
      ) : retention.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={retention.error}
          onRetry={retention.refetch}
          title="Build Result retention could not be loaded."
        />
      ) : (
        <>
          <RetentionDetails retention={retention.data} />
          <BuildResultHoldCommands
            api={api}
            buildId={buildId}
            fallbackFocusRef={headingRef}
            onRefresh={retention.refetch}
            retention={retention.data}
          />
        </>
      )}
      {retention.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={retention.error}
          fetching={retention.isFetching}
          label="Build Result retention data"
          onRetry={retention.refetch}
        />
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
          <AuditTerm
            label="Placement request"
            requestIdentity={hold.creation_audit.request_identity}
          />
          {hold.release_audit === null ? null : (
            <AuditTerm
              label="Release request"
              requestIdentity={hold.release_audit.request_identity}
            />
          )}
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

function DefinitionTerm({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function AuditTerm({ label, requestIdentity }: { label: string; requestIdentity: string }) {
  return (
    <DefinitionTerm
      label={label}
      value={<Link to={auditRequestPath(requestIdentity)}>{requestIdentity}</Link>}
    />
  );
}
