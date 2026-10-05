import { useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent, type RefObject } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
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
  const queryClient = useQueryClient();
  const [duration, setDuration] = useState<'permanent' | 'time_bounded'>('permanent');
  const [expiry, setExpiry] = useState('');
  const [reason, setReason] = useState('');
  const [validation, setValidation] = useState<string[]>([]);
  const [review, setReview] = useState<HoldReview | null>(null);
  const hold = retention.hold;

  const preparePlacement = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const parsed = parsePlacement(reason, duration, expiry);
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
          Release hold
        </button>
      ) : (
        <form className={styles.holdForm} onSubmit={preparePlacement}>
          <h3>Place retention hold</h3>
          <label className={`${commandFormStyles.field} ${styles.holdReasonField}`}>
            <span>Hold reason</span>
            <input
              className={commandFormStyles.control}
              onChange={(event) => setReason(event.target.value)}
              value={reason}
            />
          </label>
          <label className={commandFormStyles.field}>
            <span>Hold duration</span>
            <select
              className={commandFormStyles.control}
              onChange={(event) => setDuration(event.target.value as typeof duration)}
              value={duration}
            >
              <option value="permanent">Permanent</option>
              <option value="time_bounded">Time bounded</option>
            </select>
          </label>
          {duration === 'time_bounded' ? (
            <label className={commandFormStyles.field}>
              <span>Hold expiry</span>
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
            Review hold
          </button>
        </form>
      )}
      {review?.kind === 'placement' ? (
        <ConfirmedCommand
          confirmLabel="Place hold"
          consequence={placementConsequence(buildId, review.request.body)}
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
          renderSuccess={(result) => (
            <MutationSuccess action="Retention hold placed" auditKind="placement" result={result} />
          )}
          request={review.request}
          returnFocus={review.returnFocus}
          title={`Place hold on Build ${buildId}?`}
        />
      ) : review?.kind === 'release' ? (
        <ConfirmedCommand
          confirmLabel="Release hold"
          consequence={`This releases the hold on Build ${buildId}. Any overdue Build Result data becomes eligible for deletion by the next retention pass.`}
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
          renderSuccess={(result) => (
            <MutationSuccess action="Retention hold released" auditKind="release" result={result} />
          )}
          request={review.request}
          returnFocus={review.returnFocus}
          title={`Release hold on Build ${buildId}?`}
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
): { errors: string[] } | { request: PlaceBuildResultHoldRequest } {
  const errors: string[] = [];
  if (reason.length === 0) errors.push('A hold reason is required.');
  else if (reason.trim() !== reason || [...reason].some((character) => isControl(character))) {
    errors.push('The hold reason cannot contain control or surrounding whitespace.');
  } else if (new TextEncoder().encode(reason).byteLength > MAX_REASON_UTF8_BYTES) {
    errors.push(`The hold reason cannot exceed ${MAX_REASON_UTF8_BYTES} UTF-8 bytes.`);
  }
  if (duration === 'permanent') {
    return errors.length === 0 ? { request: { reason } } : { errors };
  }
  const expiresAt = new Date(expiry).getTime();
  if (!Number.isSafeInteger(expiresAt) || expiresAt <= Date.now()) {
    errors.push('Hold expiry must be a valid future date and time.');
  }
  return errors.length === 0 ? { request: { expires_at_unix_ms: expiresAt, reason } } : { errors };
}

function isControl(character: string) {
  const codePoint = character.codePointAt(0) ?? 0;
  return codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f);
}

function placementConsequence(buildId: string, request: PlaceBuildResultHoldRequest) {
  const duration =
    request.expires_at_unix_ms === undefined || request.expires_at_unix_ms === null
      ? 'permanently'
      : `until ${formatTimestamp(request.expires_at_unix_ms).display}`;
  return `This protects metadata, logs, Artifacts, and reports for Build ${buildId} ${duration}.`;
}

function MutationSuccess({
  action,
  auditKind,
  result,
}: {
  action: string;
  auditKind: 'placement' | 'release';
  result: BuildResultRetentionMutationResponse;
}) {
  const audit =
    auditKind === 'placement'
      ? result.retention.hold?.creation_audit
      : result.retention.hold?.release_audit;
  return (
    <>
      <span>
        {action} ({formatEnumLabel(result.disposition)}).
      </span>
      {audit === null || audit === undefined ? null : (
        <Link to={auditRequestPath(audit.request_identity)}>View {auditKind} audit evidence</Link>
      )}
    </>
  );
}
