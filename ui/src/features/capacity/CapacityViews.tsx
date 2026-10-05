import { useQuery } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { useRef, type ReactNode, type RefObject } from 'react';
import { Link, useParams } from 'react-router-dom';

import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { agentPoolPath, buildPath } from '../../app/routes';
import { formatBytes, formatEnumLabel, formatTimestamp } from '../../shared/display';
import { QueryFailureNotice, StaleQueryNotice } from '../../shared/QueryStateNotice';
import type { AgentPoolResource, AgentResource, CapacityApi } from './api';
import { AgentCommands } from './AgentCommands';
import styles from './CapacityViews.module.css';

export function CapacityLandingView({ title = 'Agents' }: { title?: string }) {
  return (
    <section aria-labelledby="page-title" className={styles.landing}>
      <p className={styles.eyebrow}>Capacity</p>
      <h1 id="page-title">{title}</h1>
      <p className={styles.description}>
        Select an Agent Pool or Agent in the explorer. The console presents server-published
        capacity facts and does not predict scheduling decisions.
      </p>
    </section>
  );
}

export function AgentView({ api }: { api: CapacityApi }) {
  const { agentId } = useParams<'agentId'>();
  if (agentId === undefined) return <CapacityNotFound resource="Agent" />;
  return <SelectedAgent agentId={agentId} api={api} />;
}

function SelectedAgent({ agentId, api }: { agentId: string; api: CapacityApi }) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const agent = useQuery({
    queryFn: ({ signal }) => api.getAgent(agentId, signal),
    queryKey: queryKeys.agent(agentId),
  });
  if (agent.isPending) return <CapacityLoading resource="Agent" />;
  if (agent.data === undefined) {
    return isNotFound(agent.error) ? (
      <CapacityNotFound resource="Agent" />
    ) : (
      <QueryFailureNotice
        error={agent.error}
        onRetry={agent.refetch}
        title="Agent could not be loaded."
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
        subtitle="Agent capacity"
        title={agent.data.name}
        titleRef={headingRef}
      />
      {agent.error === null ? null : (
        <StaleQueryNotice
          message="Refresh failed. Showing the last loaded Agent."
          onRetry={agent.refetch}
        />
      )}
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
  if (poolId === undefined) return <CapacityNotFound resource="Agent Pool" />;
  return <SelectedAgentPool api={api} poolId={poolId} />;
}

function SelectedAgentPool({ api, poolId }: { api: CapacityApi; poolId: string }) {
  const pool = useQuery({
    queryFn: ({ signal }) => api.getAgentPool(poolId, signal),
    queryKey: queryKeys.agentPool(poolId),
  });
  if (pool.isPending) return <CapacityLoading resource="Agent Pool" />;
  if (pool.data === undefined) {
    return isNotFound(pool.error) ? (
      <CapacityNotFound resource="Agent Pool" />
    ) : (
      <QueryFailureNotice
        error={pool.error}
        onRetry={pool.refetch}
        title="Agent Pool could not be loaded."
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
        subtitle="Agent Pool capacity"
        title={pool.data.name}
      />
      {pool.error === null ? null : (
        <StaleQueryNotice
          message="Refresh failed. Showing the last loaded Agent Pool."
          onRetry={pool.refetch}
        />
      )}
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
  state: string;
  subtitle: string;
  title: string;
  titleRef?: RefObject<HTMLHeadingElement | null>;
}) {
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
          {fetching ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      <p className={styles.identity}>{identity}</p>
      <span className={styles.status} data-state={state}>
        {formatEnumLabel(state)}
      </span>
    </header>
  );
}

function AgentFacts({ agent }: { agent: AgentResource }) {
  const lastSeen = formatTimestamp(agent.last_seen_at_unix_ms);
  return (
    <div className={styles.stack}>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Assignment and readiness</h2>
        </div>
        <dl className={styles.grid}>
          <Fact
            label="Pool"
            value={<Link to={agentPoolPath(agent.pool_id)}>{agent.pool_id}</Link>}
          />
          <Fact label="Pool version" value={agent.pool_version} />
          <Fact label="Agent version" value={agent.version} />
          <Fact
            label="Last seen"
            value={<time dateTime={lastSeen.machine ?? undefined}>{lastSeen.display}</time>}
          />
          <Fact label="Release" value={agent.inventory.agent_version} />
          <Fact
            label="Host"
            value={`${agent.inventory.host_platform.os}/${agent.inventory.host_platform.architecture}`}
          />
        </dl>
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Inventory and capacity</h2>
        </div>
        <dl className={styles.grid}>
          <Fact label="Logical CPUs" value={agent.capacity.logical_cpu_count} />
          <Fact label="Memory" value={formatBytes(agent.capacity.total_memory_bytes)} />
          <Fact label="Work disk" value={formatBytes(agent.capacity.work_disk_total_bytes)} />
          <Fact label="State disk" value={formatBytes(agent.capacity.state_disk_total_bytes)} />
          <Fact
            label="Virtualization"
            value={agent.capacity.virtualization_available ? 'Available' : 'Unavailable'}
          />
          <Fact label="Runtime count" value={agent.inventory.runtimes.length} />
        </dl>
        {agent.inventory.runtimes.length === 0 ? (
          <p className={styles.empty}>No execution runtimes were published.</p>
        ) : (
          <ul className={styles.list} aria-label="Execution runtimes">
            {agent.inventory.runtimes.map((runtime) => (
              <li
                key={`${runtime.backend}:${runtime.mode}:${runtime.platform.os}:${runtime.platform.architecture}`}
              >
                {runtime.backend} · {formatEnumLabel(runtime.mode)} · {runtime.platform.os}/
                {runtime.platform.architecture}
                {runtime.isolation === null ? '' : ` · ${formatEnumLabel(runtime.isolation)}`}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Current execution</h2>
        </div>
        {agent.current_execution === null ? (
          <p className={styles.empty}>Idle — no current execution is published.</p>
        ) : (
          <dl className={styles.grid}>
            <Fact
              label="Build"
              value={
                <Link to={buildPath(agent.current_execution.build_id)}>
                  {agent.current_execution.build_id}
                </Link>
              }
            />
            <Fact label="Attempt" value={agent.current_execution.attempt_id} />
            <Fact label="Job" value={agent.current_execution.job_id} />
            <Fact label="Lease" value={agent.current_execution.lease_id} />
            <Fact
              label="Lease state"
              value={formatEnumLabel(agent.current_execution.lease_state)}
            />
          </dl>
        )}
      </section>
    </div>
  );
}

function PoolFacts({ pool }: { pool: AgentPoolResource }) {
  const published = formatTimestamp(pool.published_at_unix_ms);
  return (
    <div className={styles.stack}>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Pool policy</h2>
        </div>
        <dl className={styles.grid}>
          <Fact label="Version" value={pool.version} />
          <Fact label="Enabled" value={pool.definition.enabled ? 'Yes' : 'No'} />
          <Fact label="Drain state" value={formatEnumLabel(pool.definition.drain_state)} />
          <Fact label="Concurrency limit" value={pool.definition.concurrency_limit} />
          <Fact label="Static capacity limit" value={pool.definition.static_capacity_limit} />
          <Fact label="Fairness" value={formatEnumLabel(pool.definition.fairness_policy)} />
          <Fact
            label="Published"
            value={<time dateTime={published.machine ?? undefined}>{published.display}</time>}
          />
          <Fact label="Admission" value={admissionLabel(pool)} />
        </dl>
      </section>
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Admission facts</h2>
        </div>
        <AdmissionFacts pool={pool} />
      </section>
    </div>
  );
}

function AdmissionFacts({ pool }: { pool: AgentPoolResource }) {
  const policy = pool.definition.admission_policy;
  if (policy.mode === 'any') {
    return <p className={styles.empty}>Any valid Agent platform may enroll.</p>;
  }
  return (
    <ul className={styles.list} aria-label="Admitted platforms">
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
              {formatEnumLabel(target.mode)}: {target.host_platform.os}/
              {target.host_platform.architecture} → {target.target_platform.os}/
              {target.target_platform.architecture}; guarantees{' '}
              {target.required_guarantees.map(formatEnumLabel).join(', ') || 'none'}
            </li>
          ))
        : null}
    </ul>
  );
}

function admissionLabel(pool: AgentPoolResource) {
  return formatEnumLabel(pool.definition.admission_policy.mode);
}

function Fact({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function CapacityLoading({ resource }: { resource: string }) {
  return (
    <section className={styles.landing}>
      <p className={styles.eyebrow}>Capacity</p>
      <h1 id="page-title">{resource}</h1>
      <p className={styles.description} role="status">
        Loading {resource}…
      </p>
    </section>
  );
}

function CapacityNotFound({ resource }: { resource: string }) {
  return (
    <section className={styles.landing}>
      <p className={styles.eyebrow}>Not found</p>
      <h1 id="page-title">{resource} not found</h1>
    </section>
  );
}

function isNotFound(error: Error | null): boolean {
  return error instanceof ManagementApiError && error.code === 'not_found';
}
