import { useState } from 'react';
import { ManagementApiError } from '../../api/client';
import {
  retainVisibleEvents,
  EXECUTION_EVENT_BACKOFF_STEPS,
  EXECUTION_EVENT_RETRY_INITIAL_MILLISECONDS,
  EXECUTION_EVENT_RETRY_MAX_MILLISECONDS,
} from '../../shared/executionEvents';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryFailureNotice,
  QueryLoadingNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import { formatTimestamp, formatEnumLabel } from '../../shared/display';
import type { Issue } from './api';
import type {
  IssueActivityApi,
  IssueJournalTarget,
  IssueJournalEvents as Events,
} from './activityApi';
import styles from './Issues.module.css';
type Props = { api: IssueActivityApi; issue: Issue; request: string; target: IssueJournalTarget };
/** Typed public facts only. Private event content has its own short-lived authorized query. */
export function IssueJournalEvents({ api, issue, request, target }: Props) {
  const { t, locale } = usePresentation();
  const [params, setParams] = useSearchParams();
  const client = useQueryClient();
  const [generation, setGeneration] = useState(0);
  const [failures, setFailures] = useState(0);
  const rawSequence = params.get('journal_event');
  const selectedSequence =
    rawSequence !== null &&
    /^\d+$/u.test(rawSequence) &&
    Number.isSafeInteger(Number(rawSequence)) &&
    Number(rawSequence) > 0
      ? Number(rawSequence)
      : null;
  const [following, setFollowing] = useState(rawSequence === null);
  const live = following && selectedSequence === null;
  const key = queryKeys.issueJournalEvents(
    issue.project_id,
    issue.id,
    request,
    target.build_execution,
    target.job_id,
    generation,
  );
  const query = useQuery<LiveEvents>({
    queryKey: key,
    gcTime: 0,
    retry: false,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: live,
    refetchInterval: (query) => {
      if (!live || finished(query.state.data)) return false;
      const error = query.state.error;
      if (
        error instanceof ManagementApiError &&
        error.status !== null &&
        error.status !== 429 &&
        error.status < 500
      )
        return false;
      return Math.min(
        EXECUTION_EVENT_RETRY_MAX_MILLISECONDS,
        EXECUTION_EVENT_RETRY_INITIAL_MILLISECONDS *
          2 **
            Math.min(
              EXECUTION_EVENT_BACKOFF_STEPS,
              Math.max(failures, query.state.data?.idle ?? 0),
            ),
      );
    },
    queryFn: async ({ signal }): Promise<LiveEvents> => {
      const previous = client.getQueryData<LiveEvents>(key);
      let page: Events;
      try {
        page = await api.journalEvents(
          issue.project_id,
          issue.id,
          request,
          target,
          previous?.cursor ?? null,
          signal,
        );
      } catch (error) {
        if (!signal.aborted)
          setFailures((count) => Math.min(EXECUTION_EVENT_BACKOFF_STEPS, count + 1));
        throw error;
      }
      setFailures(0);
      const through = Math.max(
        previous?.through ?? 0,
        ...page.events.map((e) => e.sequence),
        ...page.gaps.map((g) => g.through_sequence),
      );
      const retained = retainVisibleEvents(
        previous?.events ?? [],
        page.events.filter((e) => e.sequence > (previous?.through ?? 0)),
      );
      return {
        ...page,
        events: retained.events,
        gaps: retainVisibleEvents(
          previous?.gaps ?? [],
          page.gaps.filter((g) => g.through_sequence > (previous?.through ?? 0)),
        ).events,
        through,
        discarded: (previous?.discarded ?? 0) + retained.discarded,
        idle:
          through === (previous?.through ?? 0)
            ? Math.min(EXECUTION_EVENT_BACKOFF_STEPS, (previous?.idle ?? 0) + 1)
            : 0,
      };
    },
  });
  const data = query.data;
  return (
    <section className={styles.panel} aria-label={t('events.title')}>
      <h3>{t('events.title')}</h3>
      <div className={styles.actions}>
        <button
          onClick={() => {
            if (following) void client.cancelQueries({ queryKey: key, exact: true });
            else {
              const next = new URLSearchParams(params);
              next.delete('journal_event');
              setParams(next);
            }
            setFollowing(!following);
          }}
        >
          {t(following ? 'issues.pauseEvents' : 'issues.resumeEvents')}
        </button>
        <button onClick={() => setGeneration(generation + 1)}>{t('issues.restartEvents')}</button>
      </div>
      {query.isPending ? (
        <QueryLoadingNotice label={t('events.title')} />
      ) : !data ? (
        <QueryFailureNotice title={t('events.title')} error={query.error} onRetry={query.refetch} />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('events.title')}
            error={query.error}
            fetching={query.isFetching}
            onRetry={query.refetch}
          />
          <p>
            {formatEnumLabel(data.job.job.state, t)} ·{' '}
            {data.job.job.assignment?.assigned_agent_id ?? t('issues.notReported')}
          </p>
          {data.job.job.terminal?.failure ? (
            <p>{formatEnumLabel(data.job.job.terminal.failure, t)}</p>
          ) : null}
          <p role="status">{t(`issues.resultCoverage.${data.coverage}`)}</p>
          <p role="status">
            {t(
              finished(data)
                ? 'events.complete'
                : following
                  ? 'events.following'
                  : 'issues.eventsPaused',
            )}
          </p>
          {data.discarded ? (
            <p>
              {t('events.discarded', { visible: data.events.length, discarded: data.discarded })}
            </p>
          ) : null}
          {data.job_stale ? <p role="status">{t('issues.jobStale')}</p> : null}
          {data.gaps.map((gap) => (
            <p key={gap.from_sequence}>
              {t('issues.eventGap', { from: gap.from_sequence, through: gap.through_sequence })}
            </p>
          ))}
          <ol className={styles.list} aria-label={t('events.ordered')}>
            {data.events.map((event) => (
              <li key={event.sequence}>
                <time
                  dateTime={formatTimestamp(event.occurred_at_unix_ms, locale).machine ?? undefined}
                >
                  {formatTimestamp(event.occurred_at_unix_ms, locale).display}
                </time>
                <EventFacts facts={event.facts} />
                <button
                  aria-expanded={selectedSequence === event.sequence}
                  onClick={() => {
                    void client.cancelQueries({ queryKey: key, exact: true });
                    setFollowing(false);
                    const next = new URLSearchParams(params);
                    if (selectedSequence === event.sequence) next.delete('journal_event');
                    else next.set('journal_event', String(event.sequence));
                    setParams(next);
                  }}
                >
                  {t('issues.inspectEvent', { sequence: event.sequence })}
                </button>
              </li>
            ))}
          </ol>
        </>
      )}
      {selectedSequence !== null ? (
        <section aria-label={t('events.payload')}>
          <h4>{t('issues.inspectEvent', { sequence: selectedSequence })}</h4>
          <button
            onClick={() => {
              const next = new URLSearchParams(params);
              next.delete('journal_event');
              setParams(next);
            }}
          >
            {t('common.close')}
          </button>
          <EventDetail
            key={selectedSequence}
            api={api}
            issue={issue}
            request={request}
            target={target}
            sequence={selectedSequence}
          />
        </section>
      ) : null}
    </section>
  );
}
function EventFacts({ facts }: { facts: Events['events'][number]['facts'] }) {
  const { t } = usePresentation();
  if (facts.kind === 'not_reported') return <p>{t('issues.notReported')}</p>;
  if (facts.kind === 'lifecycle') return <p>{t(`issues.lifecycle.${facts.state}`)}</p>;
  if (facts.kind === 'resource_usage')
    return (
      <dl>
        {Object.entries(facts.usage).map(([key, value]) => (
          <div key={key}>
            <dt>{t(`issues.usage.${key as keyof typeof facts.usage}`)}</dt>
            <dd>{value ?? t('issues.notReported')}</dd>
          </div>
        ))}
      </dl>
    );
  const operation = facts.observation.operation;
  return (
    <>
      <p>
        {t(operation.kind === 'command' ? 'issues.commandOperation' : 'issues.fileOperation')} ·{' '}
        {t(`issues.operation.${facts.observation.phase}`)}
      </p>
      {operation.kind === 'file' ? (
        <>
          <p>{operation.path}</p>
          <p>{t(`issues.file.${operation.change}`)}</p>
        </>
      ) : (
        <p>{t('issues.exitCode', { code: operation.exit_code ?? t('issues.notReported') })}</p>
      )}
      {operation.failed === null ? (
        <p>{t('issues.notReported')}</p>
      ) : operation.failed ? (
        <p>{t('issues.operationFailed')}</p>
      ) : null}
    </>
  );
}
function EventDetail({ api, issue, request, target, sequence }: Props & { sequence: number }) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueJournalEvent(
      issue.project_id,
      issue.id,
      request,
      target.build_execution,
      target.job_id,
      sequence,
    ),
    gcTime: 0,
    queryFn: ({ signal }) =>
      api.journalEvent(issue.project_id, issue.id, request, target, sequence, signal),
  });
  return query.isPending ? (
    <QueryLoadingNotice label={t('events.payload')} />
  ) : !query.data ? (
    <QueryFailureNotice title={t('events.payload')} error={query.error} onRetry={query.refetch} />
  ) : (
    <pre className={styles.prose}>{JSON.stringify(query.data.items, null, 2)}</pre>
  );
}

interface LiveEvents extends Events {
  through: number;
  discarded: number;
  idle: number;
}
function finished(data: LiveEvents | undefined) {
  return (
    !!data &&
    data.coverage === 'partial' &&
    !data.job_stale &&
    !!data.job.job.terminal &&
    data.through >= data.job.log.event_cursor
  );
}
