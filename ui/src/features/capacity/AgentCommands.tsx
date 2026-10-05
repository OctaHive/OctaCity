import { useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent, type RefObject } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
import { auditRequestPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { formatEnumLabel } from '../../shared/display';
import commandFormStyles from '../../shared/OperatorCommandForm.module.css';
import { useCursorPage } from '../../shared/PagedExplorerBranch';
import type {
  AgentCommandResult,
  AgentPoolResource,
  AgentResource,
  CapacityApi,
  DrainAgentRequest,
} from './api';
import styles from './CapacityViews.module.css';

interface DrainReview {
  kind: 'drain';
  mode: DrainAgentRequest['mode'];
  returnFocus: HTMLElement;
}

interface ReassignReview {
  kind: 'reassign';
  returnFocus: HTMLElement;
  sourceName: string;
  target: AgentPoolResource;
}

type AgentCommandReview = DrainReview | ReassignReview;

/** Presents versioned Agent commands without deriving server placement eligibility. */
export function AgentCommands({
  agent,
  api,
  fallbackFocusRef,
  onRefresh,
}: {
  agent: AgentResource;
  api: CapacityApi;
  fallbackFocusRef: RefObject<HTMLElement | null>;
  onRefresh: () => unknown | Promise<unknown>;
}) {
  const queryClient = useQueryClient();
  const [drainMode, setDrainMode] = useState<DrainAgentRequest['mode']>('graceful');
  const [review, setReview] = useState<AgentCommandReview | null>(null);

  return (
    <section className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <h2>Agent commands</h2>
          <p className={styles.panelDescription}>
            Commands use the displayed Agent version and require explicit confirmation.
          </p>
        </div>
      </div>
      <div className={styles.commandGrid}>
        <div className={styles.commandCard}>
          <h3>Drain</h3>
          {agent.status === 'draining' ? (
            <p className={styles.empty}>This Agent is already draining.</p>
          ) : (
            <>
              <label className={commandFormStyles.field}>
                <span>Drain mode</span>
                <select
                  className={commandFormStyles.control}
                  onChange={(event) =>
                    setDrainMode(event.target.value as DrainAgentRequest['mode'])
                  }
                  value={drainMode}
                >
                  <option value="graceful">Graceful — finish current execution</option>
                  <option value="forced">Forced — request current execution cancellation</option>
                </select>
              </label>
              <button
                className={styles.commandButton}
                onClick={(event) =>
                  setReview({
                    kind: 'drain',
                    mode: drainMode,
                    returnFocus: event.currentTarget,
                  })
                }
                type="button"
              >
                Drain Agent
              </button>
            </>
          )}
        </div>
        {agent.current_execution === null ? (
          <IdlePoolReassignment agent={agent} api={api} onReview={setReview} />
        ) : (
          <div className={styles.commandCard}>
            <h3>Pool assignment</h3>
            <p className={styles.empty}>
              Pool reassignment is available only while the Agent is idle.
            </p>
          </div>
        )}
      </div>
      {review?.kind === 'drain' ? (
        <ConfirmedCommand
          confirmLabel={review.mode === 'graceful' ? 'Start graceful drain' : 'Start forced drain'}
          consequence={drainConsequence(agent, review.mode)}
          execute={({ headers, request }) =>
            api.drainAgent(
              request.agentId,
              { mode: request.mode },
              requireVersionedMutationHeaders(headers),
            )
          }
          fallbackFocusRef={fallbackFocusRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.agent(agent.id) },
            { queryKey: queryKeys.agentPoolAgents(agent.pool_id) },
            queryInvalidations.audit,
          ]}
          onClose={() => setReview(null)}
          onRefresh={onRefresh}
          queryClient={queryClient}
          renderSuccess={(result) => (
            <AgentCommandSuccess action="Drain accepted" auditLabel="drain" result={result} />
          )}
          request={{ agentId: agent.id, mode: review.mode }}
          returnFocus={review.returnFocus}
          title={`Drain Agent ${agent.name}?`}
          version={agent.version}
        />
      ) : review?.kind === 'reassign' ? (
        <ConfirmedCommand
          confirmLabel="Move Agent"
          consequence={`This stops assigning the Agent through ${review.sourceName} and moves ${agent.name} to ${review.target.name}. The server will reject an ineligible target Pool.`}
          execute={({ headers, request }) =>
            api.reassignAgentPool(
              request.agentId,
              { pool_id: request.targetPoolId },
              requireVersionedMutationHeaders(headers),
            )
          }
          fallbackFocusRef={fallbackFocusRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.agent(agent.id) },
            { queryKey: queryKeys.agentPoolAgents(agent.pool_id) },
            { queryKey: queryKeys.agentPoolAgents(review.target.id) },
            queryInvalidations.audit,
          ]}
          onClose={() => setReview(null)}
          onRefresh={onRefresh}
          queryClient={queryClient}
          renderSuccess={(result) => (
            <AgentCommandSuccess
              action="Pool reassigned"
              auditLabel="reassignment"
              result={result}
            />
          )}
          request={{ agentId: agent.id, targetPoolId: review.target.id }}
          returnFocus={review.returnFocus}
          title={`Move ${agent.name} to ${review.target.name}?`}
          version={agent.version}
        />
      ) : null}
    </section>
  );
}

function IdlePoolReassignment({
  agent,
  api,
  onReview,
}: {
  agent: AgentResource;
  api: CapacityApi;
  onReview: (review: ReassignReview) => void;
}) {
  const [targetPoolId, setTargetPoolId] = useState('');
  const pools = useCursorPage(queryKeys.agentPools, (cursor, signal) =>
    api.listAgentPools(cursor, signal),
  );
  const loaded = pools.data?.pages.flatMap((page) => page.items) ?? [];
  const targets = loaded.filter((pool) => pool.id !== agent.pool_id);

  function prepare(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const target = targets.find((pool) => pool.id === targetPoolId);
    const submitter = (event.nativeEvent as SubmitEvent).submitter;
    if (target === undefined || !(submitter instanceof HTMLElement)) return;
    onReview({
      kind: 'reassign',
      returnFocus: submitter,
      sourceName: loaded.find((pool) => pool.id === agent.pool_id)?.name ?? `Pool ${agent.pool_id}`,
      target,
    });
  }

  return (
    <form className={styles.commandCard} onSubmit={prepare}>
      <h3>Pool assignment</h3>
      {pools.isPending ? (
        <p className={styles.empty} role="status">
          Loading Agent Pools…
        </p>
      ) : pools.data === undefined ? (
        <div className={styles.inlineFailure} role="alert">
          <span>Agent Pools could not be loaded.</span>
          <button onClick={() => void pools.refetch()} type="button">
            Retry
          </button>
        </div>
      ) : targets.length === 0 && !pools.hasNextPage ? (
        <p className={styles.empty}>No other Agent Pool is available.</p>
      ) : (
        <>
          <label className={commandFormStyles.field}>
            <span>Target Agent Pool</span>
            <select
              className={commandFormStyles.control}
              onChange={(event) => setTargetPoolId(event.target.value)}
              value={targetPoolId}
            >
              <option value="">Select an Agent Pool</option>
              {targets.map((pool) => (
                <option key={pool.id} value={pool.id}>
                  {pool.name}
                </option>
              ))}
            </select>
          </label>
          {pools.hasNextPage ? (
            <button
              className={styles.textButton}
              disabled={pools.isFetchingNextPage}
              onClick={() => void pools.fetchNextPage()}
              type="button"
            >
              {pools.isFetchingNextPage ? 'Loading Agent Pools…' : 'Load more Agent Pools'}
            </button>
          ) : null}
          <button className={styles.commandButton} disabled={targetPoolId === ''} type="submit">
            Review pool reassignment
          </button>
        </>
      )}
    </form>
  );
}

function AgentCommandSuccess({
  action,
  auditLabel,
  result,
}: {
  action: string;
  auditLabel: string;
  result: AgentCommandResult;
}) {
  return (
    <>
      <span>
        {action} ({formatEnumLabel(result.response.disposition)}).
      </span>
      {result.requestId === null ? null : (
        <Link to={auditRequestPath(result.requestId)}>View {auditLabel} audit evidence</Link>
      )}
    </>
  );
}

function drainConsequence(agent: AgentResource, mode: DrainAgentRequest['mode']): string {
  return mode === 'graceful'
    ? `This stops new assignments to ${agent.name} and lets it finish its current execution.`
    : `This stops new assignments to ${agent.name} and requests cancellation of its current execution.`;
}
