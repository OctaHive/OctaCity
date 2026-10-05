import { useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent, type RefObject } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { auditRequestPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { formatEnumLabel, formatTimestamp } from '../../shared/display';
import commandFormStyles from '../../shared/OperatorCommandForm.module.css';
import type {
  BuildResultHoldApi,
  BuildResultRetentionMutationResponse,
  BuildResultRetentionResource,
  PlaceBuildResultHoldRequest,
} from './api';
import styles from './Builds.module.css';

const MAX_REASON_UTF8_BYTES = 512;

interface PlacementReview {
  kind: 'placement';
  request: Readonly<{ body: PlaceBuildResultHoldRequest; buildId: string }>;
  returnFocus: HTMLElement;
}

interface ReleaseReview {
  kind: 'release';
  request: Readonly<{ buildId: string; version: number }>;
  returnFocus: HTMLElement;
}

type HoldReview = PlacementReview | ReleaseReview;

/** Collects and confirms whole-Build-Result hold commands without retaining them in storage. */
export function BuildResultHoldCommands({
  api,
  buildId,
  fallbackFocusRef,
  onRefresh,
  retention,
}: {
  api: BuildResultHoldApi;
  buildId: string;
  fallbackFocusRef: RefObject<HTMLElement | null>;
  onRefresh: () => unknown | Promise<unknown>;
  retention: BuildResultRetentionResource;
}) {
  const { locale, t } = usePresentation();
  const queryClient = useQueryClient();
  const [duration, setDuration] = useState<'permanent' | 'time_bounded'>('permanent');
  const [expiry, setExpiry] = useState('');
  const [reason, setReason] = useState('');
  const [validation, setValidation] = useState<string[]>([]);
  const [review, setReview] = useState<HoldReview | null>(null);
  const hold = retention.hold;

  const preparePlacement = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const parsed = parsePlacement(reason, duration, expiry, t);
    if ('errors' in parsed) {
      setValidation(parsed.errors);
      return;
    }
    const submitter = (event.nativeEvent as SubmitEvent).submitter;
    if (!(submitter instanceof HTMLElement)) return;
    setValidation([]);
    setReview({
      kind: 'placement',
      request: { body: parsed.request, buildId },
      returnFocus: submitter,
    });
  };

  const prepareRelease = (returnFocus: HTMLElement, version: number) => {
    setReview({ kind: 'release', request: { buildId, version }, returnFocus });
  };

  return (
    <div className={styles.holdCommands}>
      {hold?.state === 'active' ? (
        <button
          className={styles.secondaryButton}
          onClick={(event) => prepareRelease(event.currentTarget, hold.version)}
          type="button"
        >
          {t('retention.release')}
        </button>
      ) : (
        <form className={styles.holdForm} onSubmit={preparePlacement}>
          <h3>{t('retention.place')}</h3>
          <label className={`${commandFormStyles.field} ${styles.holdReasonField}`}>
            <span>{t('retention.holdReason')}</span>
            <input
              className={commandFormStyles.control}
              onChange={(event) => setReason(event.target.value)}
              value={reason}
            />
          </label>
          <label className={commandFormStyles.field}>
            <span>{t('retention.holdDuration')}</span>
            <select
              className={commandFormStyles.control}
              onChange={(event) => setDuration(event.target.value as typeof duration)}
              value={duration}
            >
              <option value="permanent">{t('retention.permanent')}</option>
              <option value="time_bounded">{t('retention.timeBounded')}</option>
            </select>
          </label>
          {duration === 'time_bounded' ? (
            <label className={commandFormStyles.field}>
              <span>{t('retention.expiry')}</span>
              <input
                className={commandFormStyles.control}
                onChange={(event) => setExpiry(event.target.value)}
                type="datetime-local"
                value={expiry}
              />
            </label>
          ) : null}
          {validation.length === 0 ? null : (
            <ul className={commandFormStyles.validation} role="alert">
              {validation.map((message) => (
                <li key={message}>{message}</li>
              ))}
            </ul>
          )}
          <button className={styles.secondaryButton} type="submit">
            {t('retention.review')}
          </button>
        </form>
      )}
      {review?.kind === 'placement' ? (
        <ConfirmedCommand
          confirmLabel={t('retention.place')}
          consequence={placementConsequence(buildId, review.request.body, locale, t)}
          execute={({ headers, request }) =>
            api.placeBuildResultHold(request.buildId, request.body, headers)
          }
          fallbackFocusRef={fallbackFocusRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.buildResultRetention(buildId) },
            queryInvalidations.audit,
          ]}
          onClose={() => setReview(null)}
          queryClient={queryClient}
          renderSuccess={(result) => <MutationSuccess auditKind="placement" result={result} />}
          request={review.request}
          returnFocus={review.returnFocus}
          title={t('retention.placeTitle', { id: buildId })}
        />
      ) : review?.kind === 'release' ? (
        <ConfirmedCommand
          confirmLabel={t('retention.release')}
          consequence={t('retention.releaseConsequence', { id: buildId })}
          execute={({ headers, request }) =>
            api.releaseBuildResultHold(request.buildId, requireVersionedMutationHeaders(headers))
          }
          fallbackFocusRef={fallbackFocusRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.buildResultRetention(buildId) },
            queryInvalidations.audit,
          ]}
          onClose={() => setReview(null)}
          onRefresh={onRefresh}
          queryClient={queryClient}
          renderSuccess={(result) => <MutationSuccess auditKind="release" result={result} />}
          request={review.request}
          returnFocus={review.returnFocus}
          title={t('retention.releaseTitle', { id: buildId })}
          version={review.request.version}
        />
      ) : null}
    </div>
  );
}

function parsePlacement(
  reason: string,
  duration: 'permanent' | 'time_bounded',
  expiry: string,
  t: ReturnType<typeof usePresentation>['t'],
): { errors: string[] } | { request: PlaceBuildResultHoldRequest } {
  const errors: string[] = [];
  if (reason.length === 0) errors.push(t('retention.reasonRequired'));
  else if (reason.trim() !== reason || [...reason].some((character) => isControl(character))) {
    errors.push(t('retention.reasonControl'));
  } else if (new TextEncoder().encode(reason).byteLength > MAX_REASON_UTF8_BYTES) {
    errors.push(t('retention.reasonTooLong', { maximum: MAX_REASON_UTF8_BYTES }));
  }
  if (duration === 'permanent') {
    return errors.length === 0 ? { request: { reason } } : { errors };
  }
  const expiresAt = new Date(expiry).getTime();
  if (!Number.isSafeInteger(expiresAt) || expiresAt <= Date.now()) {
    errors.push(t('retention.expiryInvalid'));
  }
  return errors.length === 0 ? { request: { expires_at_unix_ms: expiresAt, reason } } : { errors };
}

function isControl(character: string) {
  const codePoint = character.codePointAt(0) ?? 0;
  return codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f);
}

function placementConsequence(
  buildId: string,
  request: PlaceBuildResultHoldRequest,
  locale: string,
  t: ReturnType<typeof usePresentation>['t'],
) {
  const duration =
    request.expires_at_unix_ms === undefined || request.expires_at_unix_ms === null
      ? t('retention.permanently')
      : t('retention.until', {
          timestamp: formatTimestamp(request.expires_at_unix_ms, locale).display,
        });
  return t('retention.placeConsequence', { duration, id: buildId });
}

function MutationSuccess({
  auditKind,
  result,
}: {
  auditKind: 'placement' | 'release';
  result: BuildResultRetentionMutationResponse;
}) {
  const { t } = usePresentation();
  const audit =
    auditKind === 'placement'
      ? result.retention.hold?.creation_audit
      : result.retention.hold?.release_audit;
  return (
    <>
      <span>
        {t(auditKind === 'placement' ? 'retention.placeAction' : 'retention.releaseAction')} (
        {formatEnumLabel(result.disposition, t)}).
      </span>
      {audit === null || audit === undefined ? null : (
        <Link to={auditRequestPath(audit.request_identity)}>
          {t(auditKind === 'placement' ? 'retention.placementAudit' : 'retention.releaseAudit')}
        </Link>
      )}
    </>
  );
}
