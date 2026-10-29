import { useDraftRowKeys } from './useDraftRowKeys';
import { Link } from 'react-router-dom';
import type { components } from '../../.generated/api/schema';
import { usePresentation } from '../app/presentation/PresentationProvider';
import type { MessageKey } from '../app/presentation/messages';

type Completion = components['schemas']['FactoryNodeCompletionDiagnostic'];
const fields = {
  accepted: 'triageResult.accepted',
  decision: 'triageResult.decision',
  reason: 'triageResult.reason',
  priority_assessment: 'triageResult.priority',
  work_kind: 'triageResult.workKind',
  severity: 'triageResult.severity',
  size: 'triageResult.size',
  component: 'triageResult.component',
  risk: 'triageResult.risk',
  recommended_route: 'triageResult.route',
  reproducibility: 'triageResult.reproducibility',
  admission_priority: 'triageResult.admissionPriority',
  admission_risk: 'triageResult.admissionRisk',
  status: 'triageResult.status',
  finding: 'triageResult.finding',
  capture_status: 'triageResult.capture',
} satisfies Record<string, MessageKey>;
const choices: Readonly<Partial<Record<string, MessageKey>>> = {
  eligible: 'triageResult.value.eligible',
  duplicate: 'triageResult.value.duplicate',
  out_of_scope: 'triageResult.value.out_of_scope',
  needs_input: 'triageResult.value.needs_input',
  review: 'triageResult.value.review',
  defect: 'triageResult.value.defect',
  feature_request: 'triageResult.value.feature_request',
  small: 'triageResult.value.small',
  medium: 'triageResult.value.medium',
  large: 'triageResult.value.large',
  advisory: 'triageResult.value.advisory',
  low: 'triageResult.value.low',
  high: 'triageResult.value.high',
  critical: 'triageResult.value.critical',
  server: 'triageResult.value.server',
  console: 'triageResult.value.console',
  agent: 'triageResult.value.agent',
  infrastructure: 'triageResult.value.infrastructure',
  documentation: 'triageResult.value.documentation',
  other: 'triageResult.value.other',
  escalation: 'triageResult.value.escalation',
  rejection: 'triageResult.value.rejection',
  already_fixed: 'triageResult.value.already_fixed',
  verification_only: 'triageResult.value.verification_only',
  research: 'triageResult.value.research',
  requirements: 'triageResult.value.requirements',
  development: 'triageResult.value.development',
  reproduced: 'triageResult.value.reproduced',
  not_reproduced: 'triageResult.value.not_reproduced',
  unknown: 'triageResult.value.unknown',
  needs_research: 'triageResult.value.needs_research',
  not_applicable: 'triageResult.value.not_applicable',
  completed: 'triageResult.value.completed',
  environment_unavailable: 'triageResult.value.environment_unavailable',
  matched: 'triageResult.value.matched',
  not_matched: 'triageResult.value.not_matched',
  indeterminate: 'triageResult.value.indeterminate',
  needs_review: 'triageResult.value.needs_review',
  observed: 'triageResult.value.observed',
  complete: 'triageResult.value.complete',
  cancelled: 'triageResult.value.cancelled',
  failed: 'triageResult.value.failed',
};
function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function object(value: unknown): Record<string, unknown> {
  return isObject(value) ? value : {};
}
function text(value: unknown) {
  return typeof value === 'string' || typeof value === 'number' ? String(value) : null;
}
function Rows({ value, names }: { value: unknown; names: readonly (keyof typeof fields)[] }) {
  const { t } = usePresentation();
  const row = object(value);
  return (
    <dl>
      {names.map((name) => {
        const raw = row[name];
        const label =
          name !== 'reason' && typeof raw === 'string' && Object.hasOwn(choices, raw)
            ? choices[raw]
            : undefined;
        const rendered =
          typeof raw === 'boolean'
            ? t(raw ? 'common.yes' : 'common.no')
            : label !== undefined
              ? t(label)
              : text(raw);
        return rendered === null ? null : (
          <div key={name}>
            <dt>{t(fields[name])}</dt>
            <dd>{rendered}</dd>
          </div>
        );
      })}
    </dl>
  );
}
function Pin({ value }: { value: unknown }) {
  const pin = object(value);
  return (
    <span>
      {['identity', 'version', 'digest'].map((key) => (
        <span key={key}>{text(pin[key])} </span>
      ))}
    </span>
  );
}
/** References are retained evidence identities, never model-provided URLs or payload text. */
function EvidenceReference({ value }: { value: unknown }) {
  const reference = object(value);
  return (
    <code>
      {['traceArtifactRef', 'sequence', 'call_id', 'result_digest'].map((key) => (
        <span key={key}>{text(reference[key])} </span>
      ))}
    </code>
  );
}
function Lookup({ value, evidence }: { value: unknown; evidence: unknown }) {
  const { t } = usePresentation();
  const lookup = object(value);
  const producer = object(lookup.producer);
  const references = Array.isArray(evidence) ? evidence : [];
  const observations = Array.isArray(lookup.observations) ? lookup.observations : [];
  const referenceKeys = useDraftRowKeys(references);
  const observationKeys = useDraftRowKeys(observations);
  return (
    <section aria-label={t('triageResult.lookup')}>
      <h4>{t('triageResult.lookup')}</h4>
      <Rows value={lookup} names={['status', 'capture_status']} />
      <p>
        {t(
          !lookup.capture_status
            ? 'triageResult.scopeUnavailable'
            : lookup.capture_status !== 'complete'
              ? 'triageResult.coverageIncomplete'
              : 'triageResult.coverageUnknown',
        )}
      </p>
      {typeof producer.build_id === 'string' ? (
        <Link to={`/builds/${encodeURIComponent(producer.build_id)}`}>
          {t('triageResult.lookupEvidence')}
        </Link>
      ) : null}
      <p>
        <code>
          {text(lookup.trace_artifact_id)} {text(lookup.provenance_artifact_id)}{' '}
          {text(producer.attempt_id)} {text(producer.job_id)}
        </code>
      </p>
      {Array.isArray(evidence)
        ? references.map((item, index) => (
            <p key={referenceKeys[index]}>
              <EvidenceReference value={item} />
            </p>
          ))
        : null}
      {Array.isArray(lookup.observations)
        ? observations.map((value, index) => {
            const observation = object(value);
            return (
              <dl key={observationKeys[index]}>
                <dt>{t('triageResult.source')}</dt>
                <dd>{text(observation.connection)}</dd>
                <dt>{t('triageResult.scope')}</dt>
                <dd>
                  <Pin value={observation.configuration} />
                  <Pin value={observation.resource} />
                </dd>
                <dt>{t('triageResult.tool')}</dt>
                <dd>{text(observation.tool)}</dd>
                <dt>{t('triageResult.observation')}</dt>
                <dd>{text(observation.status)}</dd>
                <dt>{t('triageResult.reference')}</dt>
                <dd>
                  <EvidenceReference value={observation.reference} />
                </dd>
              </dl>
            );
          })
        : null}
    </section>
  );
}
export function isTriageCompletion(record: Completion) {
  return (
    record.public_result !== undefined &&
    record.output_schema.version === 'v1' &&
    ['factory.triage.eligibility_accepted', 'factory.triage.classification_summary'].includes(
      record.output_schema.identity,
    )
  );
}
/** Only reviewed public schema identities are rendered. Reasoning outputs remain private. */
export function FactoryTriageResult({ record }: { record: Completion }) {
  const { t } = usePresentation();
  const value = object(record.public_result);
  const assessment = object(value.assessment);
  const dependencies = Array.isArray(assessment.dependencies) ? assessment.dependencies : [];
  const dependencyKeys = useDraftRowKeys(dependencies);
  if (!Object.keys(value).length || record.output_schema.version !== 'v1') return null;
  if (record.output_schema.identity === 'factory.triage.eligibility_accepted')
    return (
      <section aria-label={t('triageResult.project')}>
        <h3>{t('triageResult.project')}</h3>
        <Rows value={value} names={['accepted', 'decision', 'reason']} />
        <Lookup value={value.lookup} evidence={value.evidence} />
      </section>
    );
  if (record.output_schema.identity !== 'factory.triage.classification_summary') return null;
  const observed = object(object(value.evidence).observed);
  const producer = object(observed.producer);
  const scope = object(observed.scope);
  const subject = object(scope.subject);
  const policy = object(observed.policy);
  return (
    <section aria-label={t('triageResult.analyst')}>
      <h3>{t('triageResult.analyst')}</h3>
      <Rows value={value} names={['accepted', 'admission_priority', 'admission_risk']} />
      <Rows
        value={value.assessment}
        names={[
          'priority_assessment',
          'work_kind',
          'severity',
          'size',
          'component',
          'risk',
          'recommended_route',
          'reproducibility',
        ]}
      />
      {Array.isArray(assessment.dependencies) && assessment.dependencies.length ? (
        <section aria-label={t('triageResult.dependencies')}>
          <h4>{t('triageResult.dependencies')}</h4>
          {dependencies.map((value, index) => {
            const dependency = object(value);
            return (
              <dl key={dependencyKeys[index]}>
                <dt>{t('factory.run')}</dt>
                <dd>
                  {typeof dependency.run_id === 'string' ? (
                    <Link to={`/factory/${encodeURIComponent(dependency.run_id)}`}>
                      {dependency.run_id}
                    </Link>
                  ) : null}
                </dd>
                <dt>{t('triageResult.dependencyWork')}</dt>
                <dd>{text(dependency.work_id)}</dd>
                <dt>{t('triageResult.dependencyDigest')}</dt>
                <dd>{text(dependency.work_digest)}</dd>
                <dt>{t('triageResult.dependencyTerminal')}</dt>
                <dd>{text(dependency.terminal)}</dd>
              </dl>
            );
          })}
        </section>
      ) : null}
      <h4>{t('triageResult.reproductionEvidence')}</h4>
      {Object.keys(observed).length ? (
        <>
          <Rows value={observed} names={['status', 'finding']} />
          <Pin value={policy.tool} />
          <Pin value={policy.plugin} />
          <Pin value={policy.schema} />
          <Pin value={policy.command} />
          <dl>
            <dt>{t('triageResult.environment')}</dt>
            <dd>
              <Pin value={policy.environment} />
            </dd>
            <dt>{t('triageResult.criterion')}</dt>
            <dd>
              <Pin value={policy.criterion} />
            </dd>
            <dt>{t('triageResult.subject')}</dt>
            <dd>
              {[
                'project_id',
                'repository_id',
                'base_revision',
                'candidate_revision',
                'changeset_digest',
              ].map((key) => (
                <span key={key}>{text(subject[key])} </span>
              ))}
            </dd>
            <dt>{t('triageResult.input')}</dt>
            <dd>{text(scope.input_digest)}</dd>
            <dt>{t('triageResult.output')}</dt>
            <dd>{text(scope.output)}</dd>
            <dt>{t('triageResult.freshUntil')}</dt>
            <dd>{text(observed.fresh_until)}</dd>
          </dl>
          <p>
            <code>
              {text(observed.owner_input_digest)} {text(observed.record_digest)}
            </code>
          </p>
          {typeof producer.build_id === 'string' ? (
            <Link to={`/builds/${encodeURIComponent(producer.build_id)}`}>
              {t('factory.openBuild')}
            </Link>
          ) : null}
          <p>
            <code>
              {text(producer.attempt_id)} {text(producer.job_id)}
            </code>
          </p>
        </>
      ) : (
        <p>{t('triageResult.noEvidence')}</p>
      )}
    </section>
  );
}
