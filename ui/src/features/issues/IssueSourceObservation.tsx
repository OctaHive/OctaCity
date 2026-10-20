import { useEffect, useState, type RefObject } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { Issue } from './api';
import type { IssueSourcesApi, IssueSynchronization } from './sourceApi';
import { sourceAuthorityLabel, sourceFieldLabel } from './issueSourcePresentation';
type Action =
  | { kind: 'refresh'; source: Parameters<IssueSourcesApi['refresh']>[2]['source'] }
  | { kind: 'pull'; expected_generation: number; enabled: boolean };

/** Retained provenance is displayed separately from an explicit read or pull-control command. */
export function IssueSourceObservation({
  issue,
  api,
  review,
  ready,
  fallbackFocusRef,
  onReload,
}: {
  issue: Issue;
  api: IssueSourcesApi;
  review: IssueSynchronization;
  ready: boolean;
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
  onReload: () => unknown;
}) {
  const { t, locale } = usePresentation();
  const queryClient = useQueryClient();
  const [action, setAction] = useState<Action | null>(null);
  const observation = review.observation;

  return (
    <>
      {review.link ? (
        <p>
          {review.link.selection.item.resource} / {review.link.selection.item.item}
        </p>
      ) : null}
      {review.mapping ? (
        <>
          <p>{t('issueSource.mappingPublished', { version: review.mapping.version })}</p>
          {Object.entries(review.mapping.definition.fields).map(([field, authority]) => (
            <p key={field}>
              {sourceFieldLabel(field, t)}: {sourceAuthorityLabel(authority, t)}
            </p>
          ))}
        </>
      ) : null}
      {observation ? (
        <>
          <p>
            {t('issueSource.lastAccepted', {
              time: new Date(observation.accepted.observed_at).toLocaleString(locale),
            })}
          </p>
          {observation.values ? (
            <>
              <p>{t('issueSync.observedNative', { state: observation.values.state })}</p>
              <p>{observation.values.labels.join(', ')}</p>
              <p>{t('issueSync.observationScope')}</p>
              <details>
                <summary>{t('issueSync.observationProvenance')}</summary>
                <p>{observation.values.observation}</p>
                <p>
                  {observation.values.mapping.identity} · {observation.values.mapping.version} ·{' '}
                  {observation.values.mapping.digest}
                </p>
              </details>
            </>
          ) : (
            <p>{t('issueSync.noValues')}</p>
          )}
          <ObservationExpiry
            key={observation.accepted.expires_at}
            expiresAt={observation.accepted.expires_at}
          />
          <p>{t(observation.enabled ? 'issueSource.pullEnabled' : 'issueSource.pullPaused')}</p>
          {observation.pending ? <p>{t('issueSource.pendingObservation')}</p> : null}
          {observation.status ? (
            <p>{t(`issueSource.condition.${observation.status.condition}`)}</p>
          ) : null}
          <button
            type="button"
            disabled={!ready}
            onClick={() =>
              setAction({
                kind: 'pull',
                expected_generation: observation.generation,
                enabled: !observation.enabled,
              })
            }
          >
            {t(observation.enabled ? 'issueSource.pause' : 'issueSource.resume')}
          </button>
        </>
      ) : null}
      {review.link ? (
        <button
          type="button"
          disabled={!ready}
          onClick={() =>
            setAction({
              kind: 'refresh',
              source: review.mapping?.definition.source ?? review.link!.selection.source,
            })
          }
        >
          {t('issueSource.refresh')}
        </button>
      ) : null}
      {action ? (
        <ConfirmedCommand
          title={t(
            action.kind === 'refresh'
              ? 'issueSource.refresh'
              : action.enabled
                ? 'issueSource.resume'
                : 'issueSource.pause',
          )}
          confirmLabel={t('issueSource.confirmAction')}
          consequence={t(
            action.kind === 'refresh'
              ? 'issueSource.refreshConsequence'
              : 'issueSource.pullConsequence',
          )}
          request={action}
          ready={ready}
          execute={async ({ request, headers }) =>
            request.kind === 'refresh'
              ? {
                  kind: 'refresh' as const,
                  result: await api.refresh(
                    issue.project_id,
                    issue.id,
                    { source: structuredClone(request.source), continuation: null },
                    headers,
                  ),
                }
              : {
                  kind: 'pull' as const,
                  result: await api.setPull(
                    issue.project_id,
                    issue.id,
                    { expected_generation: request.expected_generation, enabled: request.enabled },
                    headers,
                  ),
                }
          }
          continuation={{
            isComplete: (outcome) =>
              outcome.kind !== 'refresh' || outcome.result.disposition !== 'scanning',
            renderPending: () => <p>{t('issueSource.refreshScanning')}</p>,
            resumeLabel: t('issueSource.continueRefresh'),
          }}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issues(issue.project_id) }]}
          onClose={() => setAction(null)}
          onRefresh={onReload}
          returnFocus={fallbackFocusRef.current!}
          fallbackFocusRef={fallbackFocusRef}
          renderSuccess={(outcome) => (
            <p>
              {outcome.kind === 'refresh'
                ? t('issueSource.refreshOutcome', { outcome: outcome.result.disposition })
                : t(outcome.result.enabled ? 'issueSource.pullEnabled' : 'issueSource.pullPaused')}
            </p>
          )}
        />
      ) : null}
    </>
  );
}

/** Receipt replacement resets only the expiry badge, preserving any frozen command intent. */
function ObservationExpiry({ expiresAt }: { expiresAt: number }) {
  const { t } = usePresentation();
  const [expired, setExpired] = useState(() => Date.now() >= expiresAt);
  useEffect(() => {
    if (expired) return;
    let timer: ReturnType<typeof setTimeout>;
    function schedule() {
      const remaining = expiresAt - Date.now();
      if (remaining <= 0) setExpired(true);
      // Browser timers have a signed 32-bit ceiling; keep the original receipt deadline.
      else timer = setTimeout(schedule, Math.min(remaining, 2_147_483_647));
    }
    timer = setTimeout(schedule, Math.min(Math.max(0, expiresAt - Date.now()), 2_147_483_647));
    return () => clearTimeout(timer);
  }, [expiresAt, expired]);
  return expired ? <p>{t('issueSource.expired')}</p> : null;
}
