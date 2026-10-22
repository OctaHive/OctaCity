import { useQueryClient } from '@tanstack/react-query';
import { useRef, useState, type FormEvent, type RefObject } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { auditRequestPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { formatEnumLabel } from '../../shared/display';
import commandFormStyles from '../../shared/OperatorCommandForm.module.css';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDismissibleLayer } from '../../shared/useDismissibleLayer';
import { useCursorPage } from '../../shared/PagedExplorerBranch';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
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

type AgentCommandReview =
  | DrainReview
  | ReassignReview
  | {
      kind: 'availability';
      enabled: boolean;
      returnFocus: HTMLElement;
    };

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
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [reassigning, setReassigning] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  useDismissibleLayer({ active: open, layerRef, triggerRef, onDismiss: () => setOpen(false) });
  const [review, setReview] = useState<AgentCommandReview | null>(null);

  return (
    <div className={styles.actions}>
      <button
        className={styles.textButton}
        type="button"
        ref={triggerRef}
        aria-expanded={open}
        onClick={() => {
          setReassigning(false);
          setOpen(!open);
        }}
      >
        {t('agentCommands.actions')}
      </button>
      {open ? (
        <div
          className={styles.actionList}
          ref={layerRef}
          role="group"
          aria-label={t('agentCommands.actions')}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              setOpen(false);
              triggerRef.current?.focus();
            }
          }}
        >
          <button
            type="button"
            className={styles.textButton}
            disabled={agent.status === 'draining' && agent.enabled === false}
            onClick={(event) => {
              setOpen(false);
              setReview({
                kind: 'availability',
                enabled: agent.enabled === false,
                returnFocus: triggerRef.current ?? event.currentTarget,
              });
            }}
          >
            {t(agent.enabled === false ? 'agentCommands.connect' : 'agentCommands.disconnect')}
          </button>
          {(['graceful', 'forced'] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              className={styles.textButton}
              disabled={agent.status === 'draining'}
              onClick={(event) => {
                setOpen(false);
                setReview({
                  kind: 'drain',
                  mode,
                  returnFocus: triggerRef.current ?? event.currentTarget,
                });
              }}
            >
              {t(
                mode === 'graceful' ? 'agentCommands.gracefulAction' : 'agentCommands.forcedAction',
              )}
            </button>
          ))}
          <button
            className={styles.textButton}
            type="button"
            onClick={() => setReassigning(!reassigning)}
          >
            {t('agentCommands.changePool')}
          </button>
          {reassigning ? (
            agent.current_execution === null ? (
              <IdlePoolReassignment
                agent={agent}
                api={api}
                onReview={(next) => {
                  setOpen(false);
                  setReview({ ...next, returnFocus: triggerRef.current ?? next.returnFocus });
                }}
              />
            ) : (
              <p className={styles.empty}>{t('agentCommands.reassignIdleOnly')}</p>
            )
          ) : null}
        </div>
      ) : null}
      {review?.kind === 'availability' ? (
        <ConfirmedCommand
          confirmLabel={t(review.enabled ? 'agentCommands.connect' : 'agentCommands.disconnect')}
          consequence={t('agentCommands.availabilityConsequence')}
          execute={({ headers, request }) =>
            api.setAgentEnabled(
              request.agentId,
              { enabled: request.enabled },
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
          renderSuccess={() => <span>{t('agentCommands.availabilityAccepted')}</span>}
          request={{ agentId: agent.id, enabled: review.enabled }}
          returnFocus={review.returnFocus}
          title={t(
            review.enabled ? 'agentCommands.connectTitle' : 'agentCommands.disconnectTitle',
            { name: agent.name },
          )}
          version={agent.version}
        />
      ) : review?.kind === 'drain' ? (
        <ConfirmedCommand
          confirmLabel={
            review.mode === 'graceful'
              ? t('agentCommands.startGraceful')
              : t('agentCommands.startForced')
          }
          consequence={drainConsequence(agent, review.mode, t)}
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
          renderSuccess={(result) => <AgentCommandSuccess action="drain" result={result} />}
          request={{ agentId: agent.id, mode: review.mode }}
          returnFocus={review.returnFocus}
          title={t('agentCommands.drainTitle', { name: agent.name })}
          version={agent.version}
        />
      ) : review?.kind === 'reassign' ? (
        <ConfirmedCommand
          confirmLabel={t('agentCommands.move')}
          consequence={t('agentCommands.moveConsequence', {
            agent: agent.name,
            source: review.sourceName,
            target: review.target.name,
          })}
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
          renderSuccess={(result) => <AgentCommandSuccess action="reassignment" result={result} />}
          request={{ agentId: agent.id, targetPoolId: review.target.id }}
          returnFocus={review.returnFocus}
          title={t('agentCommands.moveTitle', {
            agent: agent.name,
            target: review.target.name,
          })}
          version={agent.version}
        />
      ) : null}
    </div>
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
  const { t } = usePresentation();
  const [targetPoolId, setTargetPoolId] = useState('');
  const pools = useCursorPage(
    queryKeys.agentPools,
    (cursor, signal) => api.listAgentPools(cursor, signal),
    { refetchStaleOnMount: false },
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
      sourceName:
        loaded.find((pool) => pool.id === agent.pool_id)?.name ??
        t('agentCommands.poolFallback', { id: agent.pool_id }),
      target,
    });
  }

  return (
    <form className={styles.commandCard} onSubmit={prepare}>
      <h3>{t('agentCommands.poolAssignment')}</h3>
      {pools.isPending ? (
        <QueryLoadingNotice className={styles.empty} label={t('agentCommands.agentPools')} />
      ) : pools.data === undefined ? (
        <QueryFailureNotice
          className={styles.inlineFailure}
          error={pools.error}
          onRetry={pools.refetch}
          title={t('agentCommands.loadFailure')}
        />
      ) : targets.length === 0 && !pools.hasNextPage ? (
        <>
          <QueryBackgroundNotice
            error={pools.error}
            fetching={pools.isFetching && !pools.isFetchingNextPage}
            label={t('agentCommands.poolData')}
            onRetry={pools.refetch}
          />
          <QueryEmptyNotice className={styles.empty}>
            {t('agentCommands.noOtherPool')}
          </QueryEmptyNotice>
        </>
      ) : (
        <>
          <QueryBackgroundNotice
            error={pools.error}
            fetching={pools.isFetching && !pools.isFetchingNextPage}
            label={t('agentCommands.poolData')}
            onRetry={pools.isFetchNextPageError ? pools.fetchNextPage : pools.refetch}
          />
          <div className={commandFormStyles.field}>
            <span>{t('agentCommands.targetPool')}</span>
            <SelectMenu
              ariaLabel={t('agentCommands.targetPool')}
              onValueChange={setTargetPoolId}
              options={[
                { label: t('agentCommands.selectPool'), value: '' },
                ...targets.map((pool) => ({ label: pool.name, value: pool.id })),
              ]}
              value={targetPoolId}
            />
          </div>
          {pools.hasNextPage ? (
            <button
              className={styles.textButton}
              disabled={pools.isFetchingNextPage}
              onClick={() => void pools.fetchNextPage()}
              type="button"
            >
              {pools.isFetchingNextPage
                ? t('agentCommands.loadingMore')
                : t('agentCommands.loadMore')}
            </button>
          ) : null}
          <button className={styles.commandButton} disabled={targetPoolId === ''} type="submit">
            {t('agentCommands.reviewReassignment')}
          </button>
        </>
      )}
    </form>
  );
}

function AgentCommandSuccess({
  action,
  result,
}: {
  action: 'drain' | 'reassignment';
  result: AgentCommandResult;
}) {
  const { t } = usePresentation();
  return (
    <>
      <span>
        {t('agentCommands.success', {
          action:
            action === 'drain'
              ? t('agentCommands.drainAccepted')
              : t('agentCommands.poolReassigned'),
          disposition: formatEnumLabel(result.response.disposition, t),
        })}
      </span>
      {result.requestId === null ? null : (
        <Link to={auditRequestPath(result.requestId)}>
          {t(action === 'drain' ? 'agentCommands.auditDrain' : 'agentCommands.auditReassignment')}
        </Link>
      )}
    </>
  );
}

function drainConsequence(
  agent: AgentResource,
  mode: DrainAgentRequest['mode'],
  t: ReturnType<typeof usePresentation>['t'],
): string {
  return mode === 'graceful'
    ? t('agentCommands.drainConsequenceGraceful', { name: agent.name })
    : t('agentCommands.drainConsequenceForced', { name: agent.name });
}
