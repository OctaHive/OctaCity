import { useEffect, useRef, useState } from 'react';

import { ManagementApiError } from '../../api/client';
import { formatTimestamp } from '../../shared/display';
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
  const initialJob = useRef<JobEventTarget>({
    event_cursor: job.event_cursor,
    id: job.id,
    state: job.state,
    terminal: job.terminal,
  }).current;
  const [snapshot, setSnapshot] = useState(INITIAL_SNAPSHOT);

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
  }, [api, initialJob, onJobUpdate]);

  return (
    <section aria-labelledby="job-events-heading" className={styles.eventsPanel}>
      <div className={styles.eventsHeading}>
        <div>
          <p className={styles.eyebrow}>Ordered diagnostics</p>
          <h3 id="job-events-heading">Job events</h3>
        </div>
        <EventFollowerState snapshot={snapshot} />
      </div>
      {snapshot.phase === 'failed' ? <EventFailure error={snapshot.error} /> : null}
      {snapshot.discardedEventCount === 0 ? null : (
        <p className={styles.eventsNotice} role="status">
          Showing the latest {snapshot.events.length} events; {snapshot.discardedEventCount} older
          events were removed from this live view.
        </p>
      )}
      {snapshot.events.length === 0 ? (
        <p className={styles.eventsEmpty}>
          {snapshot.phase === 'complete'
            ? 'No Job events were recorded.'
            : 'Waiting for the first Job event…'}
        </p>
      ) : (
        <div className={styles.tableScroller}>
          <table aria-label="Ordered Job events" className={styles.eventTable}>
            <thead>
              <tr>
                <th scope="col">Sequence</th>
                <th scope="col">Time</th>
                <th scope="col">Kind</th>
                <th scope="col">Payload</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.events.map((event) => (
                <EventRow event={event} key={event.sequence} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function EventFollowerState({ snapshot }: { snapshot: JobEventFollowerSnapshot }) {
  let label = 'Following live events';
  if (snapshot.phase === 'complete') label = 'Event history complete';
  if (snapshot.phase === 'failed') label = 'Event follow stopped';
  if (snapshot.phase === 'retrying') {
    const seconds = Math.ceil((snapshot.retryDelayMilliseconds ?? 0) / 1_000);
    label = `Event stream interrupted. Retrying in ${seconds} ${seconds === 1 ? 'second' : 'seconds'}`;
  }
  return (
    <span aria-live="polite" className={styles.eventFollowerState} role="status">
      {label}
    </span>
  );
}

function EventFailure({ error }: { error: Error | null }) {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return (
    <div className={styles.eventFailure} role="alert">
      Job events could not be loaded.
      {requestId === null ? null : ` Request ID: ${requestId}`}
    </div>
  );
}

function EventRow({ event }: { event: JobEventResource }) {
  const occurred = formatTimestamp(event.occurred_at_unix_ms);
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
