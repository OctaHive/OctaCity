import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, Server, ServerCog } from 'lucide-react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { agentPath, agentPoolPath, CONSOLE_PATHS } from '../../app/routes';
import { formatEnumLabel } from '../../shared/display';
import {
  ExplorerBranchFailure,
  ExplorerBranchLoading,
  ExplorerBranchBackground,
  PagedExplorerBranch,
  useCursorPage,
} from '../../shared/PagedExplorerBranch';
import { useExpansionOverrides } from '../../shared/useExpansionOverrides';
import type { AgentPoolResource, AgentResource, CapacityApi } from './api';
import styles from './CapacityExplorer.module.css';

interface SelectedCapacity {
  agent: AgentResource | null;
  pool: AgentPoolResource | null;
}

const emptySelection: SelectedCapacity = { agent: null, pool: null };

/** Renders independently paginated Agent Pool -> Agent capacity navigation. */
export function CapacityExplorer({ api }: { api: CapacityApi }) {
  const { t } = usePresentation();
  const location = useLocation();
  const selectedAgentId = matchPath(CONSOLE_PATHS.agent, location.pathname)?.params.agentId ?? null;
  const selectedPoolId =
    matchPath(CONSOLE_PATHS.agentPool, location.pathname)?.params.poolId ?? null;

  return (
    <div className={styles.explorer}>
      <p className={styles.notice}>{t('explorer.capacityAuthority')}</p>
      {selectedAgentId !== null ? (
        <SelectedAgentTree api={api} agentId={selectedAgentId} />
      ) : selectedPoolId !== null ? (
        <SelectedPoolTree api={api} poolId={selectedPoolId} />
      ) : (
        <CapacityTree api={api} selection={emptySelection} />
      )}
    </div>
  );
}

function SelectedAgentTree({ api, agentId }: { api: CapacityApi; agentId: string }) {
  const { t } = usePresentation();
  const agent = useQuery({
    queryFn: ({ signal }) => api.getAgent(agentId, signal),
    queryKey: queryKeys.agent(agentId),
  });
  if (agent.isPending) return <ExplorerBranchLoading label={t('explorer.selectedAgentPath')} />;
  if (agent.data === undefined) {
    return (
      <>
        <ExplorerBranchFailure
          error={agent.error}
          label={t('explorer.selectedAgent')}
          onRetry={agent.refetch}
        />
        <CapacityTree api={api} selection={emptySelection} />
      </>
    );
  }
  return (
    <SelectedPoolForAgent
      agent={agent.data}
      api={api}
      fetchingAgent={agent.isFetching}
      retryAgent={agent.refetch}
      staleError={agent.error}
    />
  );
}

function SelectedPoolForAgent({
  agent,
  api,
  fetchingAgent,
  retryAgent,
  staleError,
}: {
  agent: AgentResource;
  api: CapacityApi;
  fetchingAgent: boolean;
  retryAgent: () => unknown;
  staleError: Error | null;
}) {
  const { t } = usePresentation();
  const pool = useQuery({
    queryFn: ({ signal }) => api.getAgentPool(agent.pool_id, signal),
    queryKey: queryKeys.agentPool(agent.pool_id),
  });
  if (pool.isPending) return <ExplorerBranchLoading label={t('explorer.selectedPool')} />;
  if (pool.data === undefined) {
    return (
      <>
        <ExplorerBranchFailure
          error={pool.error}
          label={t('explorer.selectedPool')}
          onRetry={pool.refetch}
        />
        <CapacityTree api={api} selection={{ agent, pool: null }} />
      </>
    );
  }
  return (
    <>
      <ExplorerBranchBackground
        error={staleError ?? pool.error}
        fetching={fetchingAgent || pool.isFetching}
        label={t('explorer.selectedAgentPath')}
        onRetry={() => {
          void retryAgent();
          void pool.refetch();
        }}
      />
      <CapacityTree api={api} selection={{ agent, pool: pool.data }} />
    </>
  );
}

function SelectedPoolTree({ api, poolId }: { api: CapacityApi; poolId: string }) {
  const { t } = usePresentation();
  const pool = useQuery({
    queryFn: ({ signal }) => api.getAgentPool(poolId, signal),
    queryKey: queryKeys.agentPool(poolId),
  });
  if (pool.isPending) return <ExplorerBranchLoading label={t('explorer.selectedPool')} />;
  if (pool.data === undefined) {
    return (
      <>
        <ExplorerBranchFailure
          error={pool.error}
          label={t('explorer.selectedPool')}
          onRetry={pool.refetch}
        />
        <CapacityTree api={api} selection={emptySelection} />
      </>
    );
  }
  return (
    <>
      <ExplorerBranchBackground
        error={pool.error}
        fetching={pool.isFetching}
        label={t('explorer.selectedPool')}
        onRetry={pool.refetch}
      />
      <CapacityTree api={api} selection={{ agent: null, pool: pool.data }} />
    </>
  );
}

function CapacityTree({ api, selection }: { api: CapacityApi; selection: SelectedCapacity }) {
  const { t } = usePresentation();
  const expansion = useExpansionOverrides('agent_pool');
  const pools = useCursorPage(queryKeys.agentPools, (cursor, signal) =>
    api.listAgentPools(cursor, signal),
  );
  return (
    <PagedExplorerBranch
      empty={t('explorer.noPools')}
      labels={{
        failure: t('capacity.pools'),
        loading: t('capacity.pools'),
        loadMore: t('agentCommands.loadMore'),
        loadingMore: t('agentCommands.loadingMore'),
        stale: t('capacity.pools'),
      }}
      query={pools}
      revealed={selection.pool}
    >
      {(items) => (
        <ul aria-label={t('explorer.agentCapacityHierarchy')} className={styles.tree} role="tree">
          {items.map((pool) => {
            const expanded = expansion.isExpanded(pool.id, selection.pool?.id === pool.id);
            return (
              <li aria-expanded={expanded} key={pool.id} role="treeitem">
                <div
                  className={
                    selection.pool?.id === pool.id ? `${styles.row} ${styles.selected}` : styles.row
                  }
                >
                  <button
                    aria-label={t(expanded ? 'explorer.collapse' : 'explorer.expand', {
                      name: pool.name,
                    })}
                    onClick={() => expansion.toggle(pool.id, expanded)}
                    type="button"
                  >
                    {expanded ? (
                      <ChevronDown aria-hidden="true" size={15} />
                    ) : (
                      <ChevronRight aria-hidden="true" size={15} />
                    )}
                  </button>
                  <span className={styles.iconLabel}>
                    <ServerCog aria-hidden="true" size={15} />
                    <Link
                      aria-current={selection.pool?.id === pool.id ? 'page' : undefined}
                      to={agentPoolPath(pool.id)}
                    >
                      {pool.name}
                    </Link>
                  </span>
                  <State value={pool.definition.drain_state} />
                </div>
                {expanded ? <AgentBranch api={api} pool={pool} selected={selection.agent} /> : null}
              </li>
            );
          })}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}

function AgentBranch({
  api,
  pool,
  selected,
}: {
  api: CapacityApi;
  pool: AgentPoolResource;
  selected: AgentResource | null;
}) {
  const { t } = usePresentation();
  const agents = useCursorPage(queryKeys.agentPoolAgents(pool.id), (cursor, signal) =>
    api.listAgents(pool.id, cursor, signal),
  );
  const revealed = selected?.pool_id === pool.id ? selected : null;
  return (
    <PagedExplorerBranch
      empty={t('explorer.noAgents', { name: pool.name })}
      labels={{
        failure: t('explorer.agentsInPool', { name: pool.name }),
        loading: t('explorer.agentsInPool', { name: pool.name }),
        loadMore: t('explorer.loadMoreAgents', { name: pool.name }),
        loadingMore: t('explorer.loadingAgents', { name: pool.name }),
        stale: t('explorer.agentsInPool', { name: pool.name }),
      }}
      query={agents}
      revealed={revealed}
    >
      {(items) => (
        <ul className={styles.agentGroup} role="group">
          {items.map((agent) => (
            <li key={agent.id} role="treeitem">
              <div
                className={
                  selected?.id === agent.id ? `${styles.row} ${styles.selected}` : styles.row
                }
              >
                <span aria-hidden="true" />
                <span className={styles.iconLabel}>
                  <Server aria-hidden="true" size={14} />
                  <Link
                    aria-current={selected?.id === agent.id ? 'page' : undefined}
                    to={agentPath(agent.id)}
                  >
                    {agent.name}
                  </Link>
                </span>
                <State value={agent.status} />
              </div>
            </li>
          ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}

function State({
  value,
}: {
  value: AgentResource['status'] | AgentPoolResource['definition']['drain_state'];
}) {
  const { t } = usePresentation();
  return (
    <span className={styles.status} data-state={value}>
      {formatEnumLabel(value, t)}
    </span>
  );
}
