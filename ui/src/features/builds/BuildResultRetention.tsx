import { useQuery } from '@tanstack/react-query';
import { useRef, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
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
  const { t } = usePresentation();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const retention = useQuery({
    queryFn: ({ signal }) => api.getBuildResultRetention(buildId, signal),
    queryKey: queryKeys.buildResultRetention(buildId),
  });
  return (
    <section aria-label={t('retention.label')} className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>{t('retention.eyebrow')}</p>
          <h2 ref={headingRef} tabIndex={-1}>
            {t('retention.label')}
          </h2>
        </div>
        {retention.data === undefined ? null : <HoldStatus retention={retention.data} />}
      </div>
      {retention.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label={t('retention.label')} />
      ) : retention.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={retention.error}
          onRetry={retention.refetch}
          title={t('retention.loadFailure')}
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
          label={t('retention.data')}
          onRetry={retention.refetch}
        />
      )}
    </section>
  );
}

function HoldStatus({ retention }: { retention: BuildResultRetentionResource }) {
  const { t } = usePresentation();
  const state = retention.hold?.state ?? 'absent';
  const label =
    state === 'absent'
      ? t('retention.emptyHold')
      : state === 'active'
        ? t('retention.activeHold')
        : state === 'expired'
          ? t('retention.expiredHold')
          : t('retention.releasedHold');
  const family = state === 'active' ? 'info' : state === 'expired' ? 'danger' : 'muted';
  return <span className={`${styles.status} ${styles[`state_${family}`]}`}>{label}</span>;
}

function RetentionDetails({ retention }: { retention: BuildResultRetentionResource }) {
  const { locale, t } = usePresentation();
  const hold = retention.hold;
  return (
    <div className={styles.retentionDetails}>
      {hold === null ? null : (
        <dl className={styles.holdDetails}>
          <DefinitionTerm label={t('retention.holdReason')} value={hold.reason} />
          <DefinitionTerm
            label={t('retention.expiry')}
            value={
              hold.expires_at_unix_ms === null
                ? t('retention.permanent')
                : formatTimestamp(hold.expires_at_unix_ms, locale).display
            }
          />
          <DefinitionTerm label={t('retention.version')} value={String(hold.version)} />
          <AuditTerm
            label={t('retention.placementRequest')}
            requestIdentity={hold.creation_audit.request_identity}
          />
          {hold.release_audit === null ? null : (
            <AuditTerm
              label={t('retention.releaseRequest')}
              requestIdentity={hold.release_audit.request_identity}
            />
          )}
        </dl>
      )}
      <dl className={styles.retentionGrid}>
        <RetentionComponent
          deadline={retention.deadlines.metadata_at_unix_ms}
          label={t('retention.metadata')}
          locale={locale}
          visible={retention.visibility.metadata}
        />
        <RetentionComponent
          deadline={retention.deadlines.logs_at_unix_ms}
          label={t('retention.logs')}
          locale={locale}
          visible={retention.visibility.logs}
        />
        <RetentionComponent
          deadline={retention.deadlines.artifacts_at_unix_ms}
          label={t('retention.artifacts')}
          locale={locale}
          visible={retention.visibility.artifacts}
        />
        <RetentionComponent
          deadline={retention.deadlines.reports_at_unix_ms}
          label={t('retention.reports')}
          locale={locale}
          visible={retention.visibility.reports}
        />
      </dl>
    </div>
  );
}

function RetentionComponent({
  deadline,
  label,
  locale,
  visible,
}: {
  deadline: number;
  label: string;
  locale: string;
  visible: boolean;
}) {
  const { t } = usePresentation();
  const timestamp = formatTimestamp(deadline, locale);
  return (
    <div>
      <dt>{label}</dt>
      <dd>{visible ? t('retention.visible') : t('retention.unavailable')}</dd>
      <dd className={styles.secondaryMetadata}>
        {t('retention.deadline')}{' '}
        <time dateTime={timestamp.machine ?? undefined}>{timestamp.display}</time>
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
