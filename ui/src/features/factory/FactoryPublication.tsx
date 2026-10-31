import { validationFieldLabel, LimitFailure } from './ValidationIssue';
import { ConstraintReview } from './ConstraintReview';
import { focusFactoryField } from './fieldFocus';
import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { ManagementApiError, type ManagementValidationIssue } from '../../api/client';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import {
  requireVersionedMutationHeaders,
  type ConfirmedMutationAttempt,
} from '../../shared/confirmedIntent';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { validateFlowDraft, validateConfigurationDraft, type DraftIssue } from './validateDraft';
import type { FactoryApi } from './api';

type Subject =
  | { kind: 'flow'; request: components['schemas']['CreateFactoryFlowDefinitionRequest'] }
  | { kind: 'configuration'; request: components['schemas']['CreateFactoryConfigurationRequest'] };
type Result =
  | components['schemas']['FactoryFlowDefinitionMutationResponse']
  | components['schemas']['FactoryConfigurationMutationResponse'];
type Preview =
  | { kind: 'flow'; value: components['schemas']['FactoryFlowDefinitionValidation'] }
  | { kind: 'configuration'; value: components['schemas']['FactoryConfigurationValidation'] };
export type FactoryPublicationProps = {
  api: FactoryApi;
  subject: Subject;
  version?: number;
  ready?: boolean;
};
const ISSUE_LABELS = {
  invalid_contract: 'factory.invalidContract',
  unavailable_reference: 'factory.unavailableReference',
  unbounded_cycle: 'factory.unboundedCycle',
  unreachable_node: 'factory.unreachableNode',
  incompatible_schema: 'factory.incompatibleSchema',
  budget_exceeded: 'factory.budgetExceeded',
  budget_exceeds_ceiling: 'factory.budgetExceeded',
  concurrency_exceeds_ceiling: 'factory.concurrencyExceeded',
} satisfies Record<DraftIssue['code'] | ManagementValidationIssue['code'], MessageKey>;

/** A validation proof belongs to one exact in-memory draft; the shared helper owns confirmed replay. */
export function FactoryPublication({
  api,
  subject,
  version,
  ready = true,
}: FactoryPublicationProps) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const regionId = 'factory-validation';
  const [currentVersion, setCurrentVersion] = useState(version);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [proof, setProof] = useState<{ signature: string; preview: Preview } | null>(null);
  const [review, setReview] = useState<{
    subject: Subject;
    actionSummary: string[];
    returnFocus: HTMLElement;
  } | null>(null);
  const latestSignatureRef = useRef('');
  const signature = JSON.stringify(subject);
  latestSignatureRef.current = signature;
  const local =
    subject.kind === 'flow'
      ? validateFlowDraft(subject.request.definition)
      : validateConfigurationDraft(subject.request.definition);
  const server = error instanceof ManagementApiError ? error.validationIssues : [];
  const issues = [...local, ...server];
  const valid = proof?.signature === signature && issues.length === 0;
  async function validate() {
    if (local.length) return;
    setPending(true);
    setError(null);
    setProof(null);
    const captured = signature;
    try {
      const preview: Preview =
        subject.kind === 'flow'
          ? { kind: 'flow', value: await api.validateFlow(subject.request) }
          : { kind: 'configuration', value: await api.validateConfiguration(subject.request) };
      if (latestSignatureRef.current === captured) setProof({ signature: captured, preview });
    } catch (failure) {
      if (latestSignatureRef.current === captured)
        setError(failure instanceof Error ? failure : new Error('Validation failed'));
    } finally {
      setPending(false);
    }
  }
  async function execute(attempt: ConfirmedMutationAttempt<Subject>): Promise<Result> {
    // The transport serializes a mutable copy; the helper's frozen snapshot remains untouched.
    const captured = structuredClone(attempt.request) as Subject;
    try {
      if (captured.kind === 'flow')
        return currentVersion === undefined
          ? await api.createFlow(captured.request, attempt.headers)
          : await api.replaceFlow(
              captured.request.id,
              { definition: captured.request.definition },
              requireVersionedMutationHeaders(attempt.headers),
            );
      return currentVersion === undefined
        ? await api.createConfiguration(captured.request, attempt.headers)
        : await api.replaceConfiguration(
            captured.request.id,
            { definition: captured.request.definition },
            requireVersionedMutationHeaders(attempt.headers),
          );
    } catch (failure) {
      if (failure instanceof ManagementApiError && failure.validationIssues.length) {
        setError(failure);
        setProof(null);
      }
      throw failure;
    }
  }
  async function refresh() {
    setProof(null);
    setError(null);
    try {
      const current =
        subject.kind === 'flow'
          ? await api.currentFlow(subject.request.id)
          : await api.currentConfiguration(subject.request.id);
      setCurrentVersion(
        'configuration' in current
          ? current.configuration.reference.version
          : current.flow.closure.root.version,
      );
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error('Baseline refresh failed'));
    }
  }
  const returnedErrors = proof?.preview.value.diagnostics?.errors ?? [];
  const displayedIssues = [
    ...new Map(
      [...issues, ...returnedErrors].map((issue) => [`${issue.field}:${issue.code}`, issue]),
    ).values(),
  ];
  return (
    <section aria-labelledby={`${regionId}-heading`}>
      <h3 id={`${regionId}-heading`}>{t('factory.validation')}</h3>
      {issues.length || returnedErrors.length ? (
        <ul role="alert">
          {displayedIssues.map((issue) => (
            <li key={`${issue.field}:${issue.code}`}>
              <button type="button" onClick={() => focusFactoryField(issue.field)}>
                {validationFieldLabel(issue.field, t)}
              </button>{' '}
              · {t(ISSUE_LABELS[issue.code])}
              <LimitFailure issue={issue} />
              <details>
                <summary>{t('factory.routeDetails')}</summary>
                <code>{issue.field}</code>
              </details>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? (
        <QueryFailureNotice title={t('factory.validation')} error={error} onRetry={validate} />
      ) : null}
      <button
        type="button"
        disabled={!ready || pending || local.length > 0 || review !== null}
        onClick={() => void validate()}
      >
        {t('factory.validateDraft')}
      </button>
      <button
        type="button"
        disabled={!ready || !valid || returnedErrors.length > 0 || review !== null}
        onClick={(event) => {
          if (!proof) return;
          const flow =
            proof.preview.kind === 'flow'
              ? proof.preview.value
              : proof.preview.value.configuration.flow;
          setReview({
            subject: structuredClone(subject),
            actionSummary: actionEffectsSummary(flow.closure.definitions, t),
            returnFocus: event.currentTarget,
          });
        }}
      >
        {t('factory.reviewPublication')}
      </button>
      {valid && proof ? (
        <>
          <p>{t('factory.validationPassed')}</p>
          <ValidationPreview preview={proof.preview} />
        </>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t(
            subject.kind === 'flow'
              ? 'factory.publishFlowTitle'
              : 'factory.publishConfigurationTitle',
          )}
          consequence={[t('factory.publishConsequence'), ...review.actionSummary].join('\n')}
          confirmLabel={t('factory.confirmPublish')}
          request={review.subject}
          ready={ready}
          execute={execute}
          queryClient={queryClient}
          invalidations={[
            {
              queryKey:
                subject.kind === 'flow'
                  ? queryKeys.factoryFlows(subject.request.project_id)
                  : queryKeys.factoryConfigurations(subject.request.project_id),
            },
            { queryKey: queryKeys.factoryCurrent(subject.kind, subject.request.id) },
            {
              queryKey:
                subject.kind === 'flow'
                  ? queryKeys.factoryFlowVersions(subject.request.id)
                  : queryKeys.factoryConfigurationVersions(subject.request.id),
            },
            { queryKey: queryKeys.audit },
          ]}
          {...(currentVersion === undefined ? {} : { version: currentVersion, onRefresh: refresh })}
          onClose={() => setReview(null)}
          returnFocus={review.returnFocus}
          renderSuccess={(result) => {
            const reference =
              'configuration' in result.resource
                ? result.resource.configuration.reference
                : result.resource.flow.closure.root;
            return (
              <>
                <p>{t('factory.published')}</p>
                <Link
                  to={`/factory?project_id=${encodeURIComponent(subject.request.project_id)}&${subject.kind === 'flow' ? 'flow_definition_id' : 'configuration_id'}=${encodeURIComponent(reference.id)}&${subject.kind}_version=${reference.version}`}
                >
                  {t('factory.openPublished')}
                </Link>
              </>
            );
          }}
        />
      ) : null}
    </section>
  );
}
function actionEffectsSummary(
  definitions: readonly components['schemas']['FactoryFlowDefinition'][],
  t: ReturnType<typeof usePresentation>['t'],
) {
  let hasSourceEffects = false;
  const summary = definitions.flatMap((definition) =>
    definition.nodes.flatMap((node) => {
      if (node.profile_selection) {
        const selection = node.profile_selection;
        return [
          `${node.key} · ${t('factory.stageProfiles')} · ${selection.role.id} v${selection.role.version} · ${selection.policy.mode} · ${Object.entries(
            selection.candidates,
          )
            .map(
              ([alias, candidate]) =>
                `${alias}: ${candidate.profile.definition.name} · ${candidate.profile.reference.id} v${candidate.profile.reference.version}`,
            )
            .join(
              '; ',
            )} · ${selection.failure.kind}${selection.failure.kind === 'reserve_profile' ? `: ${selection.failure.candidate}` : ''}`,
        ];
      }
      if (node.action?.action === 'transition_issue') {
        const selected = node.action
          .parameters as components['schemas']['FactoryIssueStatusActionParameters'];
        return [
          `${t('factory.issueStatusAction')} · ${node.key}: ${t('factory.issueStatusPinned', { workflow: selected.workflow_id, version: selected.workflow_version, transition: selected.transition_id, outcome: selected.outcome })}`,
        ];
      }
      if (node.action?.action !== 'set_status') return [];
      const parameters = node.action
        .parameters as components['schemas']['FactoryWorkStatusActionParameters'];
      const update = parameters?.source_update;
      if (!update) return [];
      hasSourceEffects = true;
      return [
        `${node.key}: ${update.effects.status}; ${update.source.resource.identity}; ${update.reporter.configuration.identity} · ${update.reporter.configuration.version} · ${update.reporter.configuration.digest}; ${t('factory.addEffect')}: ${update.effects.add.join(', ')}; ${t('factory.removeEffect')}: ${update.effects.remove.join(', ')}; ${t('factory.taskState')}: ${update.effects.state ?? t('factory.preserveState')}`,
      ];
    }),
  );
  return hasSourceEffects ? [t('factory.independentEffects'), ...summary] : summary;
}
function ValidationPreview({ preview }: { preview: Preview }) {
  const { t } = usePresentation();
  const flow = preview.kind === 'flow' ? preview.value : preview.value.configuration.flow;
  const diagnostics = preview.value.diagnostics;
  return (
    <section>
      <h4>{t('factory.normalized')}</h4>
      <p>{t('factory.definitionCount', { count: flow.closure.definitions.length })}</p>
      <details>
        <summary>{t('factory.admissionLimitsDetails')}</summary>
        <dl>
          {Object.entries(flow.limits.budget).map(([key, value]) => (
            <div key={key}>
              <dt>{key}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </details>
      <p>{t('factory.authorityNarrowing')}</p>
      {diagnostics ? (
        <>
          <h4>{t('factory.requiredCapabilities')}</h4>
          <ul>
            {[
              ...diagnostics.required_node_capabilities,
              ...diagnostics.required_agent_capabilities,
            ].map((reference) => (
              <li key={`${reference.identity}:${reference.version}:${reference.digest}`}>
                {reference.identity} · {reference.version}
              </li>
            ))}
            {diagnostics.required_backend_capabilities.map((capability) => (
              <li key={capability}>{capability}</li>
            ))}
          </ul>
          <ConstraintReview diagnostics={diagnostics} />
        </>
      ) : null}
    </section>
  );
}
