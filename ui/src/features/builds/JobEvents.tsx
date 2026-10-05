import { useEffect, useRef, useState } from 'react';

import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatTimestamp } from '../../shared/display';
import {
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
  StaleQueryNotice,
} from '../../shared/QueryStateNotice';
import type { BuildDiagnosticsApi, JobEventResource, JobResource } from './api';
import styles from './Builds.module.css';
import {
  followJobEvents,
  type JobEventFollowerSnapshot,
  type JobEventTarget,
} from './jobEventFollower';

const INITIAL_SNAPSHOT: JobEventFollowerSnapshot = {
  cursor: 0,
  discardedEventCount: 0,
  error: null,
  events: [],
  job: {
    event_cursor: 0,
    id: '',
    state: 'blocked',
    terminal: null,
  },
  phase: 'following',
  retryDelayMilliseconds: null,
};

/** Owns the single cancellable event follower for one selected Job. */
export function JobEvents({
  api,
  job,
  onJobUpdate,
}: {
  api: BuildDiagnosticsApi;
  job: JobResource;
  onJobUpdate: (job: JobEventTarget) => void;
}) {
  const { locale, t } = usePresentation();
  const initialJob = useRef<JobEventTarget>({
    event_cursor: job.event_cursor,
    id: job.id,
    state: job.state,
    terminal: job.terminal,
  }).current;
  const [snapshot, setSnapshot] = useState(INITIAL_SNAPSHOT);
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    void followJobEvents({
      initialJob,
      onSnapshot: (next) => {
        setSnapshot(next);
        onJobUpdate(next.job);
      },
      signal: controller.signal,
      source: api,
    });
    return () => controller.abort();
  }, [api, generation, initialJob, onJobUpdate]);

  return (
    <section aria-labelledby="job-events-heading" className={styles.eventsPanel}>
      <div className={styles.eventsHeading}>
        <div>
          <p className={styles.eyebrow}>{t('events.ordered')}</p>
          <h3 id="job-events-heading">{t('events.title')}</h3>
        </div>
        <EventFollowerState snapshot={snapshot} />
      </div>
      {snapshot.phase === 'failed' ? (
        <EventFailure
          error={snapshot.error}
          onRetry={() => setGeneration((current) => current + 1)}
          stale={snapshot.events.length > 0}
        />
      ) : null}
      {snapshot.discardedEventCount === 0 ? null : (
        <p className={styles.eventsNotice} role="status">
          {t('events.discarded', {
            discarded: snapshot.discardedEventCount,
            visible: snapshot.events.length,
          })}
        </p>
      )}
      {snapshot.events.length === 0 ? (
        snapshot.phase === 'complete' ? (
          <QueryEmptyNotice className={styles.eventsEmpty}>{t('events.noEvents')}</QueryEmptyNotice>
        ) : snapshot.phase === 'failed' ? null : (
          <QueryLoadingNotice className={styles.eventsEmpty} label={t('events.first')} />
        )
      ) : (
        <div className={styles.tableScroller}>
          <table aria-label={t('events.label')} className={styles.eventTable}>
            <thead>
              <tr>
                <th scope="col">{t('events.sequence')}</th>
                <th scope="col">{t('events.time')}</th>
                <th scope="col">{t('events.kind')}</th>
                <th scope="col">{t('events.payload')}</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.events.map((event) => (
                <EventRow event={event} key={event.sequence} locale={locale} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function EventFollowerState({ snapshot }: { snapshot: JobEventFollowerSnapshot }) {
  const { t } = usePresentation();
  let label = t('events.following');
  if (snapshot.phase === 'complete') label = t('events.complete');
  if (snapshot.phase === 'failed') label = t('events.failed');
  if (snapshot.phase === 'retrying') {
    const seconds = Math.ceil((snapshot.retryDelayMilliseconds ?? 0) / 1_000);
    label = t('events.retrying', { seconds });
  }
  return (
    <span aria-live="polite" className={styles.eventFollowerState} role="status">
      {label}
    </span>
  );
}

function EventFailure({
  error,
  onRetry,
  stale,
}: {
  error: Error | null;
  onRetry: () => unknown;
  stale: boolean;
}) {
  const { t } = usePresentation();
  return stale ? (
    <StaleQueryNotice
      className={styles.eventFailure}
      error={error}
      message={t('events.stale')}
      onRetry={onRetry}
    />
  ) : (
    <QueryFailureNotice
      className={styles.eventFailure}
      error={error}
      onRetry={onRetry}
      title={t('events.loadFailure')}
    />
  );
}

function EventRow({ event, locale }: { event: JobEventResource; locale: string }) {
  const occurred = formatTimestamp(event.occurred_at_unix_ms, locale);
  return (
    <tr>
      <th scope="row">{event.sequence}</th>
      <td>
        <time dateTime={occurred.machine ?? undefined}>{occurred.display}</time>
      </td>
      <td>{event.kind}</td>
      <td>
        <code className={styles.eventPayload}>{serializePayload(event.payload)}</code>
      </td>
    </tr>
  );
}

function serializePayload(payload: unknown): string {
  const serialized = JSON.stringify(payload);
  return serialized === undefined ? 'null' : serialized;
}
