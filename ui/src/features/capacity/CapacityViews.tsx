import { useQuery } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { useRef, type ReactNode, type RefObject } from 'react';
import { Link, useParams } from 'react-router-dom';

import { ManagementApiError } from '../../api/client';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { agentPoolPath, buildPath } from '../../app/routes';
import { formatBytes, formatEnumLabel, formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { WorkspaceWelcome } from '../../shared/WorkspaceWelcome';
import type { AgentPoolResource, AgentResource, CapacityApi } from './api';
import { AgentCommands } from './AgentCommands';
import styles from './CapacityViews.module.css';

export function CapacityLandingView({ kind = 'agents' }: { kind?: 'agents' | 'pools' }) {
  const { t } = usePresentation();
  const title = kind === 'agents' ? t('section.agents') : t('capacity.pools');
  return (
    <WorkspaceWelcome
      description={t('capacity.choose')}
      eyebrow={t('capacity.title')}
      illustrationLabel={t('workspace.agentsIllustration')}
      kind="agents"
      title={title}
    />
  );
}

export function AgentView({ api }: { api: CapacityApi }) {
  const { agentId } = useParams<'agentId'>();
  if (agentId === undefined) return <CapacityNotFound resource="agent" />;
  return <SelectedAgent agentId={agentId} api={api} />;
}

function SelectedAgent({ agentId, api }: { agentId: string; api: CapacityApi }) {
  const { t } = usePresentation();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const agent = useQuery({
    queryFn: ({ signal }) => api.getAgent(agentId, signal),
    queryKey: queryKeys.agent(agentId),
  });
  if (agent.isPending) return <CapacityLoading resource="agent" />;
  if (agent.data === undefined) {
    return isNotFound(agent.error) ? (
      <CapacityNotFound error={agent.error} onRetry={agent.refetch} resource="agent" />
    ) : (
      <QueryFailureNotice
        error={agent.error}
        onRetry={agent.refetch}
        title={t('capacity.agentLoadFailure')}
      />
    );
  }
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <CapacityHeading
        fetching={agent.isFetching}
        identity={agent.data.id}
        onRefresh={agent.refetch}
        state={agent.data.status}
        subtitle={t('capacity.agentCapacity')}
        title={agent.data.name}
        titleRef={headingRef}
      />
      <QueryBackgroundNotice
        error={agent.error}
        fetching={agent.isFetching}
        label={t('capacity.agent')}
        onRetry={agent.refetch}
      />
      <AgentFacts agent={agent.data} />
      <AgentCommands
        agent={agent.data}
        api={api}
        fallbackFocusRef={headingRef}
        onRefresh={agent.refetch}
      />
    </section>
  );
}

export function AgentPoolView({ api }: { api: CapacityApi }) {
  const { poolId } = useParams<'poolId'>();
  if (poolId === undefined) return <CapacityNotFound resource="pool" />;
  return <SelectedAgentPool api={api} poolId={poolId} />;
}

function SelectedAgentPool({ api, poolId }: { api: CapacityApi; poolId: string }) {
  const { t } = usePresentation();
  const pool = useQuery({
    queryFn: ({ signal }) => api.getAgentPool(poolId, signal),
    queryKey: queryKeys.agentPool(poolId),
  });
  if (pool.isPending) return <CapacityLoading resource="pool" />;
  if (pool.data === undefined) {
    return isNotFound(pool.error) ? (
      <CapacityNotFound error={pool.error} onRetry={pool.refetch} resource="pool" />
    ) : (
      <QueryFailureNotice
        error={pool.error}
        onRetry={pool.refetch}
        title={t('capacity.poolLoadFailure')}
      />
    );
  }
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <CapacityHeading
        fetching={pool.isFetching}
        identity={pool.data.id}
        onRefresh={pool.refetch}
        state={pool.data.definition.drain_state}
        subtitle={t('capacity.poolCapacity')}
        title={pool.data.name}
      />
      <QueryBackgroundNotice
        error={pool.error}
        fetching={pool.isFetching}
        label={t('capacity.pool')}
        onRetry={pool.refetch}
      />
      <PoolFacts pool={pool.data} />
    </section>
  );
}

function CapacityHeading({
  fetching,
  identity,
  onRefresh,
  state,
  subtitle,
  title,
  titleRef,
}: {
  fetching: boolean;
  identity: string;
  onRefresh: () => unknown;
  state: AgentResource['status'] | AgentPoolResource['definition']['drain_state'];
  subtitle: string;
  title: string;
  titleRef?: RefObject<HTMLHeadingElement | null>;
}) {
  const { t } = usePresentation();
  return (
    <header className={styles.heading}>
      <div className={styles.headingRow}>
        <div>
          <p className={styles.eyebrow}>{subtitle}</p>
          <h1 id="page-title" ref={titleRef} tabIndex={titleRef === undefined ? undefined : -1}>
            {title}
          </h1>
        </div>
        <button
          className={styles.button}
          disabled={fetching}
          onClick={() => void onRefresh()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={15} />
          {fetching ? t('common.refreshing') : t('common.refresh')}
        </button>
      </div>
      <p className={styles.identity}>{identity}</p>
      <span className={styles.status} data-state={state}>
        {formatEnumLabel(state, t)}
      </span>
    </header>
  );
}

function AgentFacts({ agent }: { agent: AgentResource }) {
  const { locale, t } = usePresentation();
  const lastSeen = formatTimestamp(agent.last_seen_at_unix_ms, locale);
  return (
    <div className={styles.stack}>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>{t('capacity.assignment')}</h2>
        </div>
        <dl className={styles.grid}>
          <Fact
            label={t('capacity.pool')}
            value={<Link to={agentPoolPath(agent.pool_id)}>{agent.pool_id}</Link>}
          />
          <Fact label={t('capacity.poolVersion')} value={agent.pool_version} />
          <Fact label={t('capacity.agentVersion')} value={agent.version} />
          <Fact
            label={t('capacity.lastSeen')}
            value={<time dateTime={lastSeen.machine ?? undefined}>{lastSeen.display}</time>}
          />
          <Fact label={t('capacity.release')} value={agent.inventory.agent_version} />
          <Fact
            label={t('capacity.host')}
            value={`${agent.inventory.host_platform.os}/${agent.inventory.host_platform.architecture}`}
          />
        </dl>
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>{t('capacity.inventory')}</h2>
        </div>
        <dl className={styles.grid}>
          <Fact label={t('capacity.logicalCpus')} value={agent.capacity.logical_cpu_count} />
          <Fact
            label={t('capacity.memory')}
            value={formatBytes(agent.capacity.total_memory_bytes, locale)}
          />
          <Fact
            label={t('capacity.workDisk')}
            value={formatBytes(agent.capacity.work_disk_total_bytes, locale)}
          />
          <Fact
            label={t('capacity.stateDisk')}
            value={formatBytes(agent.capacity.state_disk_total_bytes, locale)}
          />
          <Fact
            label={t('capacity.virtualization')}
            value={
              agent.capacity.virtualization_available
                ? t('capacity.available')
                : t('capacity.unavailable')
            }
          />
          <Fact label={t('capacity.runtimeCount')} value={agent.inventory.runtimes.length} />
        </dl>
        {agent.inventory.runtimes.length === 0 ? (
          <p className={styles.empty}>{t('capacity.noRuntimes')}</p>
        ) : (
          <ul className={styles.list} aria-label={t('capacity.runtimes')}>
            {agent.inventory.runtimes.map((runtime) => (
              <li
                key={`${runtime.backend}:${runtime.mode}:${runtime.platform.os}:${runtime.platform.architecture}`}
              >
                {runtime.backend} · {formatEnumLabel(runtime.mode, t)} · {runtime.platform.os}/
                {runtime.platform.architecture}
                {runtime.isolation === null ? '' : ` · ${formatEnumLabel(runtime.isolation, t)}`}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>{t('capacity.currentExecution')}</h2>
        </div>
        {agent.current_execution === null ? (
          <p className={styles.empty}>{t('capacity.idle')}</p>
        ) : (
          <dl className={styles.grid}>
            <Fact
              label={t('capacity.build')}
              value={
                <Link to={buildPath(agent.current_execution.build_id)}>
                  {agent.current_execution.build_id}
                </Link>
              }
            />
            <Fact label={t('capacity.attempt')} value={agent.current_execution.attempt_id} />
            <Fact label={t('capacity.job')} value={agent.current_execution.job_id} />
            <Fact label={t('capacity.lease')} value={agent.current_execution.lease_id} />
            <Fact
              label={t('capacity.leaseState')}
              value={formatEnumLabel(agent.current_execution.lease_state, t)}
            />
          </dl>
        )}
      </section>
    </div>
  );
}

function PoolFacts({ pool }: { pool: AgentPoolResource }) {
  const { locale, t } = usePresentation();
  const published = formatTimestamp(pool.published_at_unix_ms, locale);
  return (
    <div className={styles.stack}>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>{t('capacity.poolPolicy')}</h2>
        </div>
        <dl className={styles.grid}>
          <Fact label={t('capacity.version')} value={pool.version} />
          <Fact
            label={t('capacity.enabled')}
            value={pool.definition.enabled ? t('common.yes') : t('common.no')}
          />
          <Fact
            label={t('capacity.drainState')}
            value={formatEnumLabel(pool.definition.drain_state, t)}
          />
          <Fact label={t('capacity.concurrencyLimit')} value={pool.definition.concurrency_limit} />
          <Fact label={t('capacity.staticLimit')} value={pool.definition.static_capacity_limit} />
          <Fact
            label={t('capacity.fairness')}
            value={formatEnumLabel(pool.definition.fairness_policy, t)}
          />
          <Fact
            label={t('capacity.published')}
            value={<time dateTime={published.machine ?? undefined}>{published.display}</time>}
          />
          <Fact
            label={t('capacity.admission')}
            value={formatEnumLabel(pool.definition.admission_policy.mode, t)}
          />
        </dl>
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>{t('capacity.admissionFacts')}</h2>
        </div>
        <AdmissionFacts pool={pool} />
      </section>
    </div>
  );
}

function AdmissionFacts({ pool }: { pool: AgentPoolResource }) {
  const { t } = usePresentation();
  const policy = pool.definition.admission_policy;
  if (policy.mode === 'any') {
    return <p className={styles.empty}>{t('capacity.anyPlatform')}</p>;
  }
  return (
    <ul className={styles.list} aria-label={t('capacity.admittedPlatforms')}>
      {policy.platforms.map((platform) => (
        <li key={`${platform.operating_system}:${platform.architecture}`}>
          {platform.operating_system}/{platform.architecture}
        </li>
      ))}
      {policy.mode === 'execution_allowlist'
        ? policy.execution_targets.map((target) => (
            <li
              key={`${target.mode}:${target.host_platform.os}:${target.host_platform.architecture}:${target.target_platform.os}:${target.target_platform.architecture}`}
            >
              {formatEnumLabel(target.mode, t)}: {target.host_platform.os}/
              {target.host_platform.architecture} → {target.target_platform.os}/
              {target.target_platform.architecture}; {t('capacity.guarantees')}{' '}
              {target.required_guarantees
                .map((guarantee) => formatEnumLabel(guarantee, t))
                .join(', ') || t('common.none')}
            </li>
          ))
        : null}
    </ul>
  );
}

function Fact({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function CapacityLoading({ resource }: { resource: 'agent' | 'pool' }) {
  const { t } = usePresentation();
  const label = resource === 'agent' ? t('capacity.agent') : t('capacity.pool');
  return (
    <section className={styles.landing}>
      <p className={styles.eyebrow}>{t('capacity.title')}</p>
      <h1 id="page-title">{label}</h1>
      <QueryLoadingNotice className={styles.description} label={label} />
    </section>
  );
}

function CapacityNotFound({
  error = null,
  onRetry,
  resource,
}: {
  error?: Error | null;
  onRetry?: (() => unknown) | undefined;
  resource: 'agent' | 'pool';
}) {
  const { t } = usePresentation();
  const label = resource === 'agent' ? t('capacity.agent') : t('capacity.pool');
  return (
    <section className={styles.landing}>
      <p className={styles.eyebrow}>{t('route.notFound')}</p>
      <h1 id="page-title">{t('capacity.notFound', { resource: label })}</h1>
      {onRetry === undefined ? null : (
        <QueryFailureNotice
          error={error}
          onRetry={onRetry}
          title={t('capacity.wasNotFound', { resource: label })}
        />
      )}
    </section>
  );
}

function isNotFound(error: Error | null): boolean {
  return error instanceof ManagementApiError && error.code === 'not_found';
}
