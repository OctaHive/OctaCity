import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import type { components } from '../../.generated/api/schema';
import type {
  FactoryActionsApi,
  FactoryActionScope,
  FactoryActionItem,
  FactoryActionObservation,
} from '../api/factoryActions';
import { queryKeys, OPERATOR_ATTENTION_REFRESH_MILLISECONDS } from '../app/query';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { ConfirmedCommand } from './ConfirmedCommand';
import { requireVersionedMutationHeaders } from './confirmedIntent';
import { HumanGateResponse, OperatorRequest } from './FactoryNodeResponseFields';
import { QueryBackgroundNotice, QueryFailureNotice, QueryLoadingNotice } from './QueryStateNotice';
import styles from './FactoryActions.module.css';
type Description = FactoryActionItem['action'];
type Intent =
  | { kind: 'retry'; runId: string; nodeId: string }
  | {
      kind: 'response';
      runId: string;
      nodeId: string;
      body: components['schemas']['FactoryNodeResponseRequest'];
    }
  | {
      kind: 'operation';
      runId: string;
      nodeId: string;
      body: components['schemas']['FactoryNodeOperationRequest'];
    };
type Review = { request: Intent; action: Description; returnFocus: HTMLElement };
/** The scope key discards drafts on card/Project navigation; exact targets never follow an Issue head. */
export function FactoryActions({
  api,
  scope,
}: {
  api: FactoryActionsApi;
  scope: FactoryActionScope;
}) {
  return (
    <ActionQueue key={`${scope.projectId ?? ''}:${scope.issueId ?? ''}`} api={api} scope={scope} />
  );
}
function ActionQueue({ api, scope }: { api: FactoryActionsApi; scope: FactoryActionScope }) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [review, setReview] = useState<Review | null>(null);
  const after = params.get('action_after');
  const run = params.get('action_run');
  const attempt = params.get('action_attempt');
  const page = useQuery({
    queryKey: queryKeys.factoryActions(scope.projectId ?? null, scope.issueId ?? null, after),
    gcTime: 0,
    retry: false,
    queryFn: ({ signal }) => api.list(scope, after, signal),
  });
  const selected = useQuery<FactoryActionObservation>({
    queryKey: queryKeys.factoryAction(run, attempt, {
      projectId: scope.projectId ?? null,
      issueId: scope.issueId ?? null,
    }),
    enabled: run !== null && attempt !== null,
    gcTime: 0,
    retry: false,
    refetchIntervalInBackground: false,
    refetchInterval: (q) =>
      q.state.data?.state === 'recorded' && q.state.error === null
        ? OPERATOR_ATTENTION_REFRESH_MILLISECONDS
        : false,
    queryFn: ({ signal }) => api.observe(run!, attempt!, signal, scope),
  });
  const fresh =
    page.error === null && !page.isFetching && selected.error === null && !selected.isFetching;
  const action = selected.data?.state === 'waiting' ? selected.data.description : null;
  const target = action?.run_id === run && action.node_attempt_id === attempt ? action : null;
  async function refresh() {
    await Promise.all([page.refetch(), ...(run && attempt ? [selected.refetch()] : [])]);
  }
  function select(item: FactoryActionItem) {
    const next = new URLSearchParams(params);
    next.set('action_run', item.action.run_id);
    next.set('action_attempt', item.action.node_attempt_id);
    setParams(next);
  }
  function open(request: Intent, returnFocus: HTMLElement) {
    if (target && fresh) setReview({ request, action: structuredClone(target), returnFocus });
  }
  return (
    <section className={styles.queue}>
      <h2>{t('factoryActions.title')}</h2>
      <p>{t('factory.commandAuthority')}</p>
      <button type="button" onClick={refresh} disabled={page.isFetching || selected.isFetching}>
        {t('factoryActions.refresh')}
      </button>
      {!page.data ? (
        page.isPending ? (
          <QueryLoadingNotice label={t('factoryActions.title')} />
        ) : (
          <QueryFailureNotice
            title={t('factoryActions.title')}
            error={page.error}
            onRetry={refresh}
          />
        )
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('factoryActions.title')}
            fetching={page.isFetching}
            error={page.error}
            onRetry={refresh}
          />
          <p>{t('factoryActions.count', { count: page.data.total })}</p>
          {page.data.coverage === 'incomplete' ? (
            <p role="status">{t('factoryActions.incomplete')}</p>
          ) : null}
          {page.data.items.length === 0 && page.data.coverage === 'complete' ? (
            <p>{t('factoryActions.empty')}</p>
          ) : null}
          {page.data.items.map((item) => (
            <article
              className={styles.item}
              key={`${item.action.run_id}:${item.action.node_attempt_id}`}
              aria-label={`${item.action.node_key} · ${item.action.run_id}`}
            >
              <h3>{item.action.node_key}</h3>
              <Link to={`/factory-actions?project_id=${encodeURIComponent(item.project_id)}`}>
                {t('factoryActions.project')}
              </Link>
              <p>
                {t(
                  item.action.kind.kind === 'retry'
                    ? 'factory.requestNodeRetry'
                    : 'factory.reviewHumanResponse',
                )}
              </p>
              {item.association ? (
                <Link
                  to={`/projects/${encodeURIComponent(item.project_id)}/issues/${encodeURIComponent(item.association.issue_id)}?section=actions&action_run=${encodeURIComponent(item.action.run_id)}&action_attempt=${encodeURIComponent(item.action.node_attempt_id)}`}
                >
                  {t('factoryActions.task')}
                </Link>
              ) : null}
              <button
                type="button"
                disabled={page.error !== null || page.isFetching || review !== null}
                onClick={() => select(item)}
              >
                {t('factoryActions.review')}
              </button>
              <details>
                <summary>{t('issues.details')}</summary>
                <p>
                  {item.action.run_id} · {item.action.node_attempt_id} · v{item.action.run_version}
                </p>
              </details>
            </article>
          ))}
          {page.data.next_cursor ? (
            <button
              onClick={() => {
                const next = new URLSearchParams(params);
                next.set('action_after', page.data!.next_cursor!);
                setParams(next);
              }}
            >
              {t('factoryActions.more')}
            </button>
          ) : null}
          {after ? (
            <button
              onClick={() => {
                const next = new URLSearchParams(params);
                next.delete('action_after');
                setParams(next);
              }}
            >
              {t('factoryActions.first')}
            </button>
          ) : null}
        </>
      )}
      {run && attempt ? (
        <section aria-label={t('factoryActions.selected')}>
          {selected.data ? (
            <>
              <QueryBackgroundNotice
                label={t('factoryActions.selected')}
                fetching={selected.isFetching}
                error={selected.error}
                onRetry={refresh}
              />
              <p role="status">{t(`factoryActions.state.${selected.data.state}`)}</p>
              <Link
                to={`/factory/${encodeURIComponent(run)}?node_attempt_id=${encodeURIComponent(attempt)}`}
              >
                {t('factoryActions.run')}
              </Link>
              <details>
                <summary>{t('issues.details')}</summary>
                <p>
                  {run} · {attempt} · v{selected.data.run_version}
                </p>
              </details>
              {target ? (
                <fieldset disabled={!fresh || review !== null}>
                  {target.kind.kind === 'retry' ? (
                    <button
                      onClick={(e) =>
                        open({ kind: 'retry', runId: run, nodeId: attempt }, e.currentTarget)
                      }
                    >
                      {t('factory.requestNodeRetry')}
                    </button>
                  ) : target.kind.binding ? (
                    <OperatorRequest
                      binding={target.kind.binding}
                      acceptedCandidate={selected.data.accepted_candidate}
                      onReview={(body, returnFocus) =>
                        open({ kind: 'operation', runId: run, nodeId: attempt, body }, returnFocus)
                      }
                    />
                  ) : (
                    <HumanGateResponse
                      key={`${run}:${attempt}:${target.run_version}`}
                      outcomes={target.kind.outcomes}
                      schemas={target.kind.schemas}
                      onReview={(body, returnFocus) =>
                        open({ kind: 'response', runId: run, nodeId: attempt, body }, returnFocus)
                      }
                    />
                  )}
                </fieldset>
              ) : null}
            </>
          ) : selected.isPending ? (
            <QueryLoadingNotice label={t('factoryActions.selected')} />
          ) : (
            <QueryFailureNotice
              title={t('factoryActions.selected')}
              error={selected.error}
              onRetry={refresh}
            />
          )}
        </section>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('factory.confirmRunCommand')}
          consequence={t('factory.nodeConsequence', {
            run: review.action.run_id,
            node: review.action.node_key,
            flow: review.action.flow_run_id,
            cycle: review.action.cycle_id,
            version: review.action.run_version,
            outcome:
              review.request.kind === 'response'
                ? review.request.body.outcome
                : review.request.kind === 'operation'
                  ? review.request.body.disposition
                  : 'retry',
          })}
          confirmLabel={t('factory.confirmCommand')}
          request={review.request}
          version={review.action.run_version}
          returnFocus={review.returnFocus}
          ready={
            fresh &&
            target?.run_version === review.action.run_version &&
            target.node_attempt_id === review.action.node_attempt_id
          }
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryActionsRoot },
            {
              queryKey: queryKeys.factoryAction(
                review.action.run_id,
                review.action.node_attempt_id,
              ),
            },
            { queryKey: queryKeys.factoryRun(review.action.run_id) },
            { queryKey: queryKeys.factoryDiagnosticRoot(review.action.run_id) },
          ]}
          execute={(intent) => {
            const request = structuredClone(intent.request);
            const headers = requireVersionedMutationHeaders(intent.headers);
            switch (request.kind) {
              case 'retry':
                return api.retryNode(request.runId, request.nodeId, headers);
              case 'operation':
                return api.requestNodeOperation(
                  request.runId,
                  request.nodeId,
                  request.body,
                  headers,
                );
              case 'response':
                return api.respondNode(request.runId, request.nodeId, request.body, headers);
            }
          }}
          onRefresh={refresh}
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <>
              <p>
                {t(
                  selected.data?.state === 'consumed'
                    ? 'factoryActions.state.consumed'
                    : 'factoryActions.recorded',
                )}
              </p>
              <p>
                {result.disposition} · v{result.version}
              </p>
            </>
          )}
        />
      ) : null}
    </section>
  );
}
