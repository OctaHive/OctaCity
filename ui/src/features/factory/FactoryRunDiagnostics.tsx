import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { FlowGraphOverview } from './FlowGraphOverview';
import { FactoryRunSummary } from './FactoryRunSummary';
import { FactoryFlowRunGraph } from './FactoryFlowRunGraph';
import { FactoryPools } from './FactoryPools';
import { FactoryRunCommands } from './FactoryRunCommands';
import styles from './Factory.module.css';
import { useDraftRowKeys } from './useDraftRowKeys';
import type { FactoryApi, FactoryRun, FactoryDiagnostic, FactoryDiagnosticCollection } from './api';

export const DIAGNOSTIC_LABELS = {
  'flow-runs': 'factory.diag.flowRuns',
  'workflow-cycles': 'factory.diag.cycles',
  'node-attempts': 'factory.diag.nodes',
  builds: 'factory.diag.builds',
  budgets: 'factory.diag.budgets',
  audit: 'factory.diag.audit',
  calls: 'factory.diag.calls',
  work: 'factory.diag.work',
  'node-completions': 'factory.diag.completions',
  'node-stops': 'factory.diag.stops',
  'decision-signals': 'factory.diag.signals',
  changesets: 'factory.diag.changesets',
  evidence: 'factory.diag.evidence',
  'review-plans': 'factory.diag.plans',
  assessments: 'factory.diag.assessments',
  decisions: 'factory.diag.decisions',
  escalations: 'factory.diag.escalations',
  delivery: 'factory.diag.delivery',
  promotion: 'factory.diag.promotion',
  reporting: 'factory.diag.reporting',
} satisfies Record<FactoryDiagnosticCollection, MessageKey>;

const RECORD_FIELDS = {
  flow_run: ['id', 'definition', 'parent'],
  workflow_cycle: ['id', 'flow_run_id', 'number', 'predecessor', 'source_attempt_id'],
  node_attempt: [
    'id',
    'flow_run_id',
    'cycle_id',
    'node_key',
    'node_kind',
    'number',
    'input_digest',
    'execution',
    'retry_of',
    'started_at',
    'deadline',
  ],
  build: ['id', 'build_id', 'attempt_id', 'jobs', 'node_attempt_id', 'operation_id'],
  budget: ['id', 'run_version', 'recorded_at', 'usage'],
  audit: [
    'id',
    'operation',
    'outcome',
    'actor_kind',
    'actor_identity_digest',
    'request_identity_digest',
    'recorded_at',
  ],
  work: ['id', 'external_identity', 'configuration', 'subject', 'priority', 'risk'],
  node_completion: [
    'id',
    'flow_run_id',
    'cycle_id',
    'node_attempt_id',
    'node_key',
    'outcome',
    'output_schema',
    'output_digest',
    'usage',
    'observed_at',
  ],
  node_stop: ['id', 'node_attempt_id', 'reason', 'execution', 'usage', 'observed_at'],
  call: ['node', 'profile'],
  domain_fact: [
    'id',
    'flow_run_id',
    'cycle_id',
    'node_attempt_id',
    'input_digest',
    'observed_at',
    'facts',
  ],
} satisfies Record<FactoryDiagnostic['kind'], readonly string[]>;
const FACT_FIELDS = {
  candidate: ['subject'],
  changeset: ['id', 'subject', 'node_attempt_id'],
  evidence: ['id', 'subject', 'changeset_id', 'digest', 'item_count'],
  review_plan: ['id', 'subject', 'evidence_id', 'digest', 'purpose', 'required_quorum', 'deadline'],
  assessment: [
    'id',
    'subject',
    'plan_id',
    'digest',
    'evaluator',
    'model',
    'prompt_digest',
    'outcome',
  ],
  decision: ['id', 'subject', 'outcome', 'explanation'],
  delivery: ['id', 'subject', 'decision_id', 'state'],
  escalation: ['id', 'subject'],
  reporting: ['id', 'subject', 'number', 'state'],
  promotion: [
    'subject',
    'plan_digest',
    'input_digest',
    'environment_contract',
    'deployment',
    'outcome',
    'started_at',
    'finished_at',
  ],
  decision_signal: [
    'id',
    'subject',
    'request_id',
    'receipt_digest',
    'input_digest',
    'frozen_input_digest',
    'question_set',
    'probability_semantics',
    'provider',
    'adapter',
    'model',
    'policy',
    'purpose',
    'mode',
    'fallback',
    'answers',
    'state',
    'consumption',
  ],
} satisfies Record<
  Extract<FactoryDiagnostic, { kind: 'domain_fact' }>['record']['facts']['kind'],
  readonly string[]
>;
// Only published metadata fields are traversed. Payloads, commands, prompts and transfer URLs have no renderer.
const NESTED_FIELDS = new Set([
  'id',
  'kind',
  'node',
  'node_key',
  'node_kind',
  'number',
  'flow_run_id',
  'cycle_id',
  'factory_run_id',
  'node_attempt_id',
  'identity',
  'version',
  'digest',
  'definition_digest',
  'project_id',
  'repository_id',
  'base_revision',
  'candidate_revision',
  'changeset_digest',
  'outcome',
  'state',
  'subject',
  'decision_id',
  'attempts',
  'elapsed_millis',
  'tokens',
  'cost_micro_units',
  'output_bytes',
  'max_attempts',
  'max_elapsed_millis',
  'max_tokens',
  'max_cost_micro_units',
  'max_output_bytes',
  'baseline',
  'recommendation',
  'final_disposition',
  'Route',
  'question',
  'value',
  'FiniteChoice',
  'BoundedScore',
  'selected',
  'selected_probability',
  'runner_up_probability',
  'score',
  'confidence',
  'plan_id',
  'assessment_ids',
  'signal_receipt_ids',
  'reasons',
  'input_digest',
  'policy_version',
  'policy_digest',
  'PolicySatisfied',
  'RequiredEvidenceMissing',
  'RequiredEvidenceFailed',
  'RequiredEvidenceIndeterminate',
  'RequiredAssessmentMissing',
  'AssessmentIndeterminate',
  'SeverityThresholdExceeded',
  'evaluator',
  'severity',
  'QuorumNotMet',
  'observed',
  'required',
  'build_configuration',
  'model_or_tool',
  'plugin',
  'tool',
  'task_digest',
  'context_id',
  'context_digest',
  'permission_digest',
  'operation_id',
  'retry_of',
  'started_at',
  'deadline',
  'requested_at',
  'exact',
]);

/** Displays an allowlisted safe projection; arbitrary runtime object members are ignored. */
export function SafeFactoryMetadata({
  value,
  fields,
  depth = 0,
}: {
  value: unknown;
  fields?: readonly string[];
  depth?: number;
}) {
  if (value === null || value === undefined) return <span>—</span>;
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean')
    return <span>{String(value)}</span>;
  if (depth >= 8) return <span>…</span>;
  if (Array.isArray(value)) return <MetadataList items={value} depth={depth} />;
  if (typeof value !== 'object') return null;
  const record = value as Record<string, unknown>;
  const keys = fields ?? Object.keys(record).filter((key) => NESTED_FIELDS.has(key));
  return (
    <dl>
      {keys
        .filter((key) => Object.hasOwn(record, key))
        .map((key) => (
          <div key={key}>
            <dt>{key.replaceAll('_', ' ')}</dt>
            <dd>
              <SafeFactoryMetadata value={record[key]} depth={depth + 1} />
            </dd>
          </div>
        ))}
    </dl>
  );
}
function MetadataList({ items, depth }: { items: readonly unknown[]; depth: number }) {
  const keys = useDraftRowKeys(items);
  return (
    <ul>
      {items.map((item, index) => (
        <li key={keys[index]}>
          <SafeFactoryMetadata value={item} depth={depth + 1} />
        </li>
      ))}
    </ul>
  );
}

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
  const stops = useFactoryDiagnostics(api, run.id, 'node-stops');
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
        ] as const
      )
        .filter(([kind]) => kind !== collection)
        .map(([kind, query]) => (
          <DiagnosticDependencyNotice key={kind} query={query} label={t(DIAGNOSTIC_LABELS[kind])} />
        ))}
      <FactoryRunCommands
        api={api}
        run={run}
        configuration={configuration.data}
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
                    item.kind === 'domain_fact' ? (
                      <>
                        <p>
                          {item.record.facts.kind === 'decision_signal'
                            ? t('factory.diag.signals')
                            : item.record.facts.kind === 'decision'
                              ? t('factory.authoritativeDecision')
                              : item.record.facts.kind}
                        </p>
                        {item.record.facts.kind === 'decision_signal' &&
                        (!item.record.facts.purpose ||
                          !item.record.facts.mode ||
                          !item.record.facts.fallback) ? (
                          <p>{t('factory.historicalSignalMetadata')}</p>
                        ) : null}
                        <SafeFactoryMetadata
                          value={item.record.facts}
                          fields={FACT_FIELDS[item.record.facts.kind]}
                        />
                        <SafeFactoryMetadata
                          value={item.record}
                          fields={[
                            'id',
                            'flow_run_id',
                            'cycle_id',
                            'node_attempt_id',
                            'input_digest',
                            'observed_at',
                          ]}
                        />
                      </>
                    ) : (
                      <SafeFactoryMetadata value={item.record} fields={RECORD_FIELDS[item.kind]} />
                    )
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
