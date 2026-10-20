import type { components, operations } from '../../.generated/api/schema';
type FactoryDiagnostic = components['schemas']['FactoryDiagnosticPage']['items'][number];
type FactoryDiagnosticCollection =
  operations['listFactoryDiagnostics']['parameters']['path']['collection'];
import type { MessageKey } from '../app/presentation/messages';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { useDraftRowKeys } from './useDraftRowKeys';
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

/** Shares the existing privacy allowlists across Factory and exact Issue result views. */
export function DiagnosticRecordDetails({ item }: { item: FactoryDiagnostic }) {
  const { t } = usePresentation();
  return item.kind === 'domain_fact' ? (
    <>
      <p>
        {item.record.facts.kind === 'decision_signal'
          ? t('factory.diag.signals')
          : item.record.facts.kind === 'decision'
            ? t('factory.authoritativeDecision')
            : item.record.facts.kind}
      </p>
      {item.record.facts.kind === 'decision_signal' &&
      (!item.record.facts.purpose || !item.record.facts.mode || !item.record.facts.fallback) ? (
        <p>{t('factory.historicalSignalMetadata')}</p>
      ) : null}
      <SafeFactoryMetadata value={item.record.facts} fields={FACT_FIELDS[item.record.facts.kind]} />
      <SafeFactoryMetadata
        value={item.record}
        fields={['id', 'flow_run_id', 'cycle_id', 'node_attempt_id', 'input_digest', 'observed_at']}
      />
    </>
  ) : (
    <SafeFactoryMetadata value={item.record} fields={RECORD_FIELDS[item.kind]} />
  );
}
