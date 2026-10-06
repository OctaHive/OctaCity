import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, Server, ServerCog } from 'lucide-react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { agentPath, agentPoolPath, CONSOLE_PATHS } from '../../app/routes';
import { formatEnumLabel } from '../../shared/display';
import {
  ExplorerBranchFailure,
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

interface SelectedCapacityFailure {
  error: Error | null;
  label: string;
  onRetry: () => unknown;
}

interface SelectedCapacityState {
  background: SelectedCapacityFailure | null;
  failure: SelectedCapacityFailure | null;
  selection: SelectedCapacity;
}

const emptySelection: SelectedCapacity = { agent: null, pool: null };

/** Renders independently paginated Agent Pool -> Agent capacity navigation. */
export function CapacityExplorer({ api }: { api: CapacityApi }) {
  const { t } = usePresentation();
  const location = useLocation();
  const selectedAgentId = matchPath(CONSOLE_PATHS.agent, location.pathname)?.params.agentId ?? null;
  const selectedPoolId =
    matchPath(CONSOLE_PATHS.agentPool, location.pathname)?.params.poolId ?? null;
  const selected = useSelectedCapacity(api, selectedAgentId, selectedPoolId);

  return (
    <div className={styles.explorer}>
      <p className={styles.notice}>{t('explorer.capacityAuthority')}</p>
      {selected.failure === null ? null : <ExplorerBranchFailure {...selected.failure} />}
      {selected.background === null ? null : (
        <ExplorerBranchBackground {...selected.background} fetching={false} />
      )}
      <CapacityTree api={api} selection={selected.selection} />
    </div>
  );
}

function useSelectedCapacity(
  api: CapacityApi,
  selectedAgentId: string | null,
  selectedPoolId: string | null,
): SelectedCapacityState {
  const { t } = usePresentation();
  const agent = useQuery<AgentResource, Error>({
    enabled: selectedAgentId !== null,
    placeholderData: (previous: AgentResource | undefined) => previous,
    queryFn: ({ signal }) => api.getAgent(requireSelection(selectedAgentId), signal),
    queryKey:
      selectedAgentId === null
        ? queryKeys.disabledExplorerDetail('agent')
        : queryKeys.agent(selectedAgentId),
  });
  const resolvedPoolId =
    selectedPoolId ?? (selectedAgentId === null ? null : (agent.data?.pool_id ?? null));
  const pool = useQuery({
    enabled: resolvedPoolId !== null,
    queryFn: ({ signal }) => api.getAgentPool(requireSelection(resolvedPoolId), signal),
    queryKey:
      resolvedPoolId === null
        ? queryKeys.disabledExplorerDetail('agent-pool')
        : queryKeys.agentPool(resolvedPoolId),
  });

  if (selectedAgentId === null && selectedPoolId === null) {
    return { background: null, failure: null, selection: emptySelection };
  }
  if (selectedAgentId !== null && !agent.isPending && agent.data === undefined) {
    return {
      background: null,
      failure: {
        error: agent.error,
        label: t('explorer.selectedAgent'),
        onRetry: agent.refetch,
      },
      selection: emptySelection,
    };
  }
  if (resolvedPoolId !== null && !pool.isPending && pool.data === undefined) {
    return {
      background: null,
      failure: {
        error: pool.error,
        label: t('explorer.selectedPool'),
        onRetry: pool.refetch,
      },
      selection: { agent: agent.data ?? null, pool: null },
    };
  }

  const stale = [agent, pool].find((query) => query.error !== null && query.data !== undefined);
  return {
    background:
      stale === undefined
        ? null
        : {
            error: stale.error,
            label:
              selectedAgentId === null
                ? t('explorer.selectedPool')
                : t('explorer.selectedAgentPath'),
            onRetry: stale.refetch,
          },
    failure: null,
    selection: {
      agent: agent.isPlaceholderData ? null : (agent.data ?? null),
      pool: pool.data ?? null,
    },
  };
}

function CapacityTree({ api, selection }: { api: CapacityApi; selection: SelectedCapacity }) {
  const { t } = usePresentation();
  const expansion = useExpansionOverrides('agent_pool');
  const pools = useCursorPage(
    queryKeys.agentPools,
    (cursor, signal) => api.listAgentPools(cursor, signal),
    { refetchStaleOnMount: false },
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
  const agents = useCursorPage(
    queryKeys.agentPoolAgents(pool.id),
    (cursor, signal) => api.listAgents(pool.id, cursor, signal),
    { refetchStaleOnMount: false },
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

function requireSelection(selection: string | null): string {
  if (selection === null) throw new Error('selected capacity identifier is unavailable');
  return selection;
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
