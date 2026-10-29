import { isTriageCompletion } from '../../shared/FactoryTriageResult';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { FlowGraphOverview } from './FlowGraphOverview';
import { FactoryRunSummary } from '../../shared/FactoryRunSummary';
import { FactoryFlowRunGraph } from './FactoryFlowRunGraph';
import { FactoryPools } from './FactoryPools';
import { FactoryRunCommands } from './FactoryRunCommands';
import styles from './Factory.module.css';
import {
  DIAGNOSTIC_LABELS,
  DiagnosticRecordDetails,
  SafeFactoryMetadata,
} from '../../shared/FactoryDiagnosticRecord';
export { DIAGNOSTIC_LABELS, SafeFactoryMetadata } from '../../shared/FactoryDiagnosticRecord';
import type { FactoryApi, FactoryRun, FactoryDiagnostic, FactoryDiagnosticCollection } from './api';

export function useFactoryDiagnostics(
  api: FactoryApi,
  runId: string,
  collection: FactoryDiagnosticCollection,
  initialCursor: string | null = null,
) {
  return useCursorPage(
    queryKeys.factoryDiagnostics(runId, collection, initialCursor),
    async (cursor, signal) => {
      const page = await api.listDiagnostics(runId, collection, cursor, signal);
      return {
        ...page,
        items: page.items.map((item) => ({
          ...item,
          id: item.kind === 'call' ? item.record.node.id : item.record.id,
        })),
      };
    },
    { initialCursor },
  );
}

/** Supporting reads keep their own failure and refresh state even when their journal is not selected. */
function DiagnosticDependencyNotice({
  query,
  label,
}: {
  query: ReturnType<typeof useFactoryDiagnostics>;
  label: string;
}) {
  if (query.isPending) return <QueryLoadingNotice label={label} />;
  if (query.data === undefined)
    return <QueryFailureNotice title={label} error={query.error} onRetry={query.refetch} />;
  return (
    <QueryBackgroundNotice
      label={label}
      error={query.error}
      fetching={query.isFetching}
      onRetry={query.refetch}
    />
  );
}

/** The pinned closure describes configuration; journal collections describe authoritative execution. */
export function FactoryRunDiagnostics({ api, run }: { api: FactoryApi; run: FactoryRun }) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [params, setParams] = useSearchParams();
  const selected = params.get('diagnostic');
  const collection =
    (Object.keys(DIAGNOSTIC_LABELS) as FactoryDiagnosticCollection[]).find(
      (key) => key === selected,
    ) ?? 'node-attempts';
  const records = useFactoryDiagnostics(api, run.id, collection, params.get('diagnostic_after'));
  const nodes = useFactoryDiagnostics(api, run.id, 'node-attempts');
  const flows = useFactoryDiagnostics(api, run.id, 'flow-runs');
  const completions = useFactoryDiagnostics(api, run.id, 'node-completions');
  const selections = useFactoryDiagnostics(api, run.id, 'profile-selections');
  const stops = useFactoryDiagnostics(api, run.id, 'node-stops');
  const effects = useFactoryDiagnostics(api, run.id, 'effect-delivery');
  const configuration = useQuery({
    queryKey: queryKeys.factoryConfiguration(run.configuration_id, run.configuration_version),
    queryFn: ({ signal }) =>
      api.getConfiguration(run.configuration_id, run.configuration_version, signal),
  });
  const knownCompletions =
    completions.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.kind === 'node_completion') ?? [];
  const knownStops =
    stops.data?.pages.flatMap((page) => page.items).filter((item) => item.kind === 'node_stop') ??
    [];
  const diagnosticParams = (
    item: FactoryDiagnostic,
    id: string,
    query = records,
    selectedCollection = collection,
  ) => {
    const next = new URLSearchParams(params);
    next.set('diagnostic', selectedCollection);
    next.set('diagnostic_id', id);
    const page = query.data?.pages.findIndex((page) => page.items.some((row) => row.id === id));
    const cursor = page === undefined || page < 0 ? null : query.data?.pageParams[page];
    if (typeof cursor === 'string') next.set('diagnostic_after', cursor);
    else next.delete('diagnostic_after');
    if (item.kind === 'node_attempt') {
      next.set('flow_run_id', item.record.flow_run_id);
      next.set('cycle_id', item.record.cycle_id);
      next.set('node_attempt_id', item.record.id);
    }
    if (item.kind === 'flow_run') {
      next.set('flow_run_id', item.record.id);
      next.delete('cycle_id');
      next.delete('node_attempt_id');
    }
    return next;
  };
  const provenanceParams = (
    record: Extract<FactoryDiagnostic, { kind: 'call' }>['record']['node'],
  ) => {
    const next = new URLSearchParams(params);
    next.delete('diagnostic_after');
    next.set('diagnostic', 'calls');
    next.set('diagnostic_id', record.id);
    next.set('flow_run_id', record.flow_run_id);
    next.set('cycle_id', record.cycle_id);
    next.set('node_attempt_id', record.id);
    return next.toString();
  };
  const inspect = (item: FactoryDiagnostic, id: string) => setParams(diagnosticParams(item, id));
  const label = t(DIAGNOSTIC_LABELS[collection]);
  return (
    <section className={styles.runSections}>
      <FactoryRunSummary run={run} />
      {(
        [
          ['node-attempts', nodes],
          ['node-completions', completions],
          ['node-stops', stops],
          ['profile-selections', selections],
        ] as const
      )
        .filter(([kind]) => kind !== collection)
        .map(([kind, query]) => (
          <DiagnosticDependencyNotice key={kind} query={query} label={t(DIAGNOSTIC_LABELS[kind])} />
        ))}
      {knownCompletions.some((item) => isTriageCompletion(item.record)) ? (
        <section className={styles.panel} aria-label={t('triageResult.results')}>
          <h2>{t('triageResult.results')}</h2>
          <PagedExplorerBranch
            query={completions}
            empty={t('factory.noDiagnostics')}
            labels={{
              failure: t('triageResult.results'),
              loading: t('triageResult.results'),
              stale: t('triageResult.results'),
              loadMore: t('factory.moreDiagnostics'),
              loadingMore: t('triageResult.results'),
            }}
          >
            {(items) =>
              items
                .filter(
                  (item) => item.kind === 'node_completion' && isTriageCompletion(item.record),
                )
                .map((item) =>
                  item.kind === 'node_completion' ? (
                    <article key={item.id} className={styles.recordCard}>
                      <DiagnosticRecordDetails item={item} />
                      {item.record.result_provenance?.map((call) => (
                        <Link key={call.node.id} to={{ search: provenanceParams(call.node) }}>
                          {t('triageResult.provenance')}
                        </Link>
                      ))}
                    </article>
                  ) : null,
                )
            }
          </PagedExplorerBranch>
          <h3>{t('triageResult.effect')}</h3>
          <PagedExplorerBranch
            query={effects}
            empty={t('triageResult.noDelivery')}
            labels={{
              failure: t('triageResult.effect'),
              loading: t('triageResult.effect'),
              stale: t('triageResult.effect'),
              loadMore: t('factory.moreDiagnostics'),
              loadingMore: t('triageResult.effect'),
            }}
          >
            {(items) =>
              items
                .filter((item) => item.kind === 'effect_delivery')
                .map((item) => (
                  <article key={item.id} className={styles.recordCard}>
                    <DiagnosticRecordDetails item={item} />
                  </article>
                ))
            }
          </PagedExplorerBranch>
        </section>
      ) : null}
      <FactoryRunCommands
        api={api}
        run={run}
        configuration={configuration.data}
        profileSelections={
          (collection === 'profile-selections' ? records : selections).data?.pages
            .flatMap((page) => page.items)
            .filter((item) => item.kind === 'profile_selection')
            .map((item) => item.record) ?? []
        }
        attempts={
          (collection === 'node-attempts' ? records : nodes).data?.pages
            .flatMap((page) => page.items)
            .filter((item) => item.kind === 'node_attempt')
            .map((item) => item.record) ?? []
        }
        flowRuns={
          flows.data?.pages
            .flatMap((page) => page.items)
            .filter((item) => item.kind === 'flow_run')
            .map((item) => item.record) ?? []
        }
        onRefresh={() =>
          Promise.all([
            queryClient.invalidateQueries({ queryKey: queryKeys.factoryRun(run.id) }),
            queryClient.invalidateQueries({ queryKey: queryKeys.factoryDiagnosticRoot(run.id) }),
          ])
        }
      />
      <details className={styles.diagnosticDetails}>
        <summary>{t('factory.exactIdentities')}</summary>
        <SafeFactoryMetadata
          value={run.diagnostics}
          fields={['base_revision', 'candidate', 'latest_decision', 'escalation', 'delivery']}
        />
      </details>
      <section className={styles.panel} aria-label={t('factory.executionJournal')}>
        <h2>{t('factory.executionJournal')}</h2>
        <SelectMenu
          ariaLabel={t('factory.diagnosticCollection')}
          value={collection}
          onValueChange={(value) => {
            const next = new URLSearchParams(params);
            next.set('diagnostic', value);
            next.delete('diagnostic_id');
            next.delete('diagnostic_after');
            setParams(next);
          }}
          options={(Object.keys(DIAGNOSTIC_LABELS) as FactoryDiagnosticCollection[]).map(
            (value) => ({ value, label: t(DIAGNOSTIC_LABELS[value]) }),
          )}
        />
        <h3>{label}</h3>
        <p>{t('factory.boundedDiagnostics')}</p>
        {collection === 'decision-signals' ? <p>{t('factory.signalObservation')}</p> : null}
        <PagedExplorerBranch
          query={records}
          empty={t('factory.noDiagnostics')}
          labels={{
            failure: label,
            loading: label,
            stale: label,
            loadMore: t('factory.moreDiagnostics'),
            loadingMore: label,
          }}
        >
          {(items) => (
            <>
              {collection === 'node-attempts' ? (
                <table>
                  <caption>{t('factory.diag.nodes')}</caption>
                  <thead>
                    <tr>
                      <th>{t('factory.node')}</th>
                      <th>{t('factory.executionState')}</th>
                      <th>{t('factory.nodeKind')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items
                      .filter((item) => item.kind === 'node_attempt')
                      .map((item) => (
                        <tr key={item.id}>
                          <th scope="row">
                            <button type="button" onClick={() => inspect(item, item.id)}>
                              {item.record.node_key} · {item.record.number}
                            </button>
                          </th>
                          <td>
                            {knownCompletions.some((row) => row.record.node_attempt_id === item.id)
                              ? t('factory.recordCompleted')
                              : (knownStops.find((row) => row.record.node_attempt_id === item.id)
                                  ?.record.reason.kind ??
                                (item.record.retryable
                                  ? t('factory.retryEligible')
                                  : item.record.active
                                    ? t(
                                        item.record.node_kind === 'join'
                                          ? 'factory.pendingJoin'
                                          : 'factory.activeAttempt',
                                      )
                                    : completions.error || stops.error
                                      ? t('factory.executionDetailsUnavailable')
                                      : completions.isPending || stops.isPending
                                        ? t('factory.executionDetailsLoading')
                                        : t('factory.recordedAttempt')))}
                          </td>
                          <td>{item.record.node_kind}</td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              ) : null}
              {items.map((item) => (
                <section className={styles.recordCard} key={item.id}>
                  <button
                    type="button"
                    aria-pressed={params.get('diagnostic_id') === item.id}
                    onClick={() => inspect(item, item.id)}
                  >
                    {t('factory.inspectDiagnostic', { id: item.id })}
                  </button>
                  {item.kind === 'build' ? (
                    <Link to={`/builds/${encodeURIComponent(item.record.build_id)}`}>
                      {t('factory.openBuild')}
                    </Link>
                  ) : null}
                  {params.get('diagnostic_id') === item.id ? (
                    <DiagnosticRecordDetails item={item} />
                  ) : null}
                </section>
              ))}
            </>
          )}
        </PagedExplorerBranch>
      </section>
      <section className={styles.panel}>
        <h2>{t('factory.nestedFlows')}</h2>
        <PagedExplorerBranch
          query={flows}
          empty={t('factory.noDiagnostics')}
          labels={{
            failure: t('factory.diag.flowRuns'),
            loading: t('factory.diag.flowRuns'),
            stale: t('factory.diag.flowRuns'),
            loadMore: t('factory.moreDiagnostics'),
            loadingMore: t('factory.diag.flowRuns'),
          }}
        >
          {(items) => (
            <FactoryFlowRunGraph
              getFlowHref={(flow) =>
                `/factory/${encodeURIComponent(run.id)}?${diagnosticParams({ kind: 'flow_run', record: flow }, flow.id, flows, 'flow-runs')}`
              }
              flows={items.filter((item) => item.kind === 'flow_run').map((item) => item.record)}
            />
          )}
        </PagedExplorerBranch>
      </section>
      {configuration.isPending ? (
        <QueryLoadingNotice label={t('factory.pinnedClosure')} />
      ) : configuration.data === undefined ? (
        <QueryFailureNotice
          title={t('factory.pinnedClosure')}
          error={configuration.error}
          onRetry={configuration.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('factory.pinnedClosure')}
            error={configuration.error}
            fetching={configuration.isFetching}
            onRetry={configuration.refetch}
          />
          <FactoryPools api={api} configuration={configuration.data} />
          <details className={styles.diagnosticDetails}>
            <summary>{t('factory.pinnedClosure')}</summary>
            {configuration.data.configuration.flow.closure.definitions.map((definition) => (
              <section key={`${definition.reference.id}:${definition.reference.version}`}>
                <SafeFactoryMetadata
                  value={definition.reference}
                  fields={['id', 'version', 'digest']}
                />
                <FlowGraphOverview value={definition} readOnly />
                <table>
                  <caption>{t('factory.pinnedNodes')}</caption>
                  <thead>
                    <tr>
                      <th>{t('factory.node')}</th>
                      <th>{t('factory.nodeKind')}</th>
                      <th>{t('factory.outcomes')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {definition.nodes.map((node) => (
                      <tr key={node.key}>
                        <th scope="row">{node.key}</th>
                        <td>{node.kind}</td>
                        <td>{node.outcomes.map((outcome) => outcome.key).join(' · ')}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <h4>{t('factory.contextProjections')}</h4>
                <ul>
                  {definition.context_projections.map((edge) => (
                    <li
                      key={`${edge.predecessor}:${edge.outcome}:${edge.successor}:${edge.selections.join(',')}`}
                    >
                      {edge.predecessor} → {edge.successor} · {edge.outcome} ·{' '}
                      {edge.selections.join(' · ')}
                    </li>
                  ))}
                </ul>
                <h4>{t('factory.declaredPermissions')}</h4>
                <p className={styles.supportingText}>{t('factory.permissionCeilingHelp')}</p>
                {definition.nodes.map((node) => (
                  <section key={node.key}>
                    <h5>{node.key}</h5>
                    <p>
                      {t('factory.permissionCounts', {
                        plugins: node.permissions.plugins.length,
                        tools: node.permissions.tools.length,
                        hosts: node.permissions.network_hosts.length,
                      })}
                    </p>
                    <SafeFactoryMetadata
                      value={node.permissions.resources}
                      fields={[
                        'cpu_millis',
                        'memory_bytes',
                        'disk_bytes',
                        'process_count',
                        'elapsed_millis',
                      ]}
                    />
                  </section>
                ))}
              </section>
            ))}
          </details>
        </>
      )}
    </section>
  );
}
