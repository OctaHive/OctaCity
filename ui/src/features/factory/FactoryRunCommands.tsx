import styles from './Factory.module.css';
import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import type { FactoryApi, FactoryRun, FactoryConfigurationDetails, FactoryDiagnostic } from './api';
import type { components } from '../../../.generated/api/schema';
import { SelectMenu } from '../../shared/SelectMenu';
import { FactoryValueInput, initialFactoryValue, validFactoryValue } from './FactoryValueInput';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';

export type FactoryAttempt = Extract<FactoryDiagnostic, { kind: 'node_attempt' }>['record'];
export type FactoryFlowRun = Extract<FactoryDiagnostic, { kind: 'flow_run' }>['record'];
type Props = {
  api: FactoryApi;
  run: FactoryRun;
  configuration?: FactoryConfigurationDetails | undefined;
  attempts: readonly FactoryAttempt[];
  flowRuns: readonly FactoryFlowRun[];
  onRefresh: () => Promise<unknown>;
};
type Intent =
  | { kind: 'cancel'; runId: string }
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
    }
  | { kind: 'retry'; runId: string; nodeId: string };
type Review = { request: Intent; version: number; returnFocus: HTMLElement; consequence: string };

/** Commands capture exact targets and versions once; only REST can accept a disposition. */
export function FactoryRunCommands({
  api,
  run,
  onRefresh,
  configuration,
  attempts,
  flowRuns,
}: Props) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [review, setReview] = useState<Review | null>(null);
  const [params, setParams] = useSearchParams();
  const selectedId = params.get('node_attempt_id');
  const terminal = run.state === 'cancelled' || run.state === 'completed';
  const candidate = run.diagnostics.candidate;
  const decision = run.diagnostics.latest_decision;
  const acceptedCandidate =
    candidate !== null &&
    decision?.outcome === 'accept' &&
    candidate.candidate_revision === decision.subject.candidate_revision &&
    candidate.changeset_digest === decision.subject.changeset_digest &&
    (['project_id', 'repository_id', 'base_revision'] as const).every(
      (key) => candidate.exact[key] === decision.subject.exact[key],
    );
  const current = attempts.filter(
    (attempt) => attempt.retryable || (attempt.active && attempt.node_kind === 'human_gate'),
  );
  const selected = selectedId ? current.find((attempt) => attempt.id === selectedId) : current[0];
  const flow = flowRuns.find((flow) => flow.id === selected?.flow_run_id);
  const definition = configuration?.configuration.flow.closure.definitions.find(
    (definition) =>
      definition.reference.id === flow?.definition.id &&
      definition.reference.version === flow.definition.version &&
      definition.reference.digest === flow.definition.digest,
  );
  const node = definition?.nodes.find((node) => node.key === selected?.node_key);
  const openReview = (request: Intent, returnFocus: HTMLElement, consequence: string) =>
    setReview({ request, returnFocus, consequence, version: run.version });
  return (
    <section className={styles.panel}>
      <h2>{t('factory.runCommands')}</h2>
      <p>{t('factory.commandAuthority')}</p>
      <button
        type="button"
        disabled={terminal || review !== null}
        onClick={(event) =>
          setReview({
            request: { kind: 'cancel', runId: run.id },
            version: run.version,
            returnFocus: event.currentTarget,
            consequence: t('factory.cancelConsequence', { id: run.id, version: run.version }),
          })
        }
      >
        {t('factory.requestCancellation')}
      </button>
      {current.length > 0 ? (
        <fieldset disabled={terminal || review !== null}>
          <SelectMenu
            ariaLabel={t('factory.commandNode')}
            value={selected?.id ?? ''}
            onValueChange={(value) => {
              const next = new URLSearchParams(params);
              next.set('node_attempt_id', value);
              setParams(next);
            }}
            options={current.map((attempt) => ({
              value: attempt.id,
              label: `${attempt.node_key} · ${attempt.number} · ${attempt.cycle_id}`,
            }))}
          />
          {selected && node?.kind === 'human_gate' && !node.operator_requests && configuration ? (
            <HumanGateResponse
              key={selected.id}
              node={node}
              configuration={configuration}
              onReview={(body, target) =>
                openReview(
                  { kind: 'response', runId: run.id, nodeId: selected.id, body },
                  target,
                  t('factory.nodeConsequence', {
                    run: run.id,
                    node: selected.node_key,
                    flow: selected.flow_run_id,
                    cycle: selected.cycle_id,
                    version: run.version,
                    outcome: body.outcome,
                  }),
                )
              }
            />
          ) : null}
          {selected && node?.kind === 'human_gate' && node.operator_requests ? (
            <OperatorRequest
              key={selected.id}
              binding={node.operator_requests}
              acceptedCandidate={acceptedCandidate}
              onReview={(body, target) =>
                openReview(
                  { kind: 'operation', runId: run.id, nodeId: selected.id, body },
                  target,
                  `${t('factory.nodeConsequence', { run: run.id, node: selected.node_key, flow: selected.flow_run_id, cycle: selected.cycle_id, version: run.version, outcome: body.disposition })} · ${run.diagnostics.candidate?.exact.repository_id ?? ''} · ${run.diagnostics.base_revision} · ${run.diagnostics.candidate?.candidate_revision ?? ''}`,
                )
              }
            />
          ) : null}
          {selected?.retryable ? (
            <button
              type="button"
              onClick={(event) =>
                openReview(
                  { kind: 'retry', runId: run.id, nodeId: selected.id },
                  event.currentTarget,
                  t('factory.nodeConsequence', {
                    run: run.id,
                    node: selected.node_key,
                    flow: selected.flow_run_id,
                    cycle: selected.cycle_id,
                    version: run.version,
                    outcome: 'retry',
                  }),
                )
              }
            >
              {t('factory.requestNodeRetry')}
            </button>
          ) : null}
        </fieldset>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('factory.confirmRunCommand')}
          consequence={review.consequence}
          confirmLabel={t('factory.confirmCommand')}
          request={review.request}
          version={review.version}
          returnFocus={review.returnFocus}
          queryClient={queryClient}
          invalidations={[
            { queryKey: queryKeys.factoryRun(run.id) },
            { queryKey: queryKeys.factoryDiagnosticRoot(run.id) },
            { queryKey: queryKeys.factoryRuns(run.project_id, {}) },
            { queryKey: queryKeys.factoryRuns(null, {}) },
            { queryKey: queryKeys.factoryPoolRoot(run.project_id) },
            { queryKey: queryKeys.audit },
          ]}
          execute={(attempt) => {
            const request = structuredClone(attempt.request) as Intent;
            const headers = requireVersionedMutationHeaders(attempt.headers);
            switch (request.kind) {
              case 'cancel':
                return api.cancelRun(request.runId, headers);
              case 'response':
                return api.respondNode(request.runId, request.nodeId, request.body, headers);
              case 'operation':
                return api.requestNodeOperation(
                  request.runId,
                  request.nodeId,
                  request.body,
                  headers,
                );
              case 'retry':
                return api.retryNode(request.runId, request.nodeId, headers);
            }
          }}
          onClose={() => setReview(null)}
          onRefresh={onRefresh}
          renderSuccess={(result) => (
            <>
              <p>
                {result.cancellation_requested
                  ? t('factory.cancellationRequested')
                  : t('factory.commandRecorded')}
              </p>
              <p>
                {result.state} · v{result.version} · {result.disposition}
              </p>
              <Link to={`/factory/${encodeURIComponent(result.run_id)}?diagnostic=audit`}>
                {t('command.viewAudit')}
              </Link>
            </>
          )}
        />
      ) : null}
    </section>
  );
}

function OperatorRequest({
  binding,
  acceptedCandidate,
  onReview,
}: {
  binding: NonNullable<components['schemas']['FactoryFlowNodeDefinition']['operator_requests']>;
  acceptedCandidate: boolean;
  onReview: (
    body: components['schemas']['FactoryNodeOperationRequest'],
    target: HTMLElement,
  ) => void;
}) {
  const { t } = usePresentation();
  const schemas = FACTORY_AUTHORING_SCHEMAS.FactoryNodeOperationRequest.oneOf;
  const reasonMaximum = Math.min(...schemas.map((schema) => schema.properties.reason.maxLength));
  const schema = schemas.find((schema) => schema.properties.operation.const === binding.operation);
  const dispositionSchema = schema?.properties.disposition;
  const supported =
    dispositionSchema && 'enum' in dispositionSchema
      ? dispositionSchema.enum
      : dispositionSchema && 'const' in dispositionSchema
        ? [dispositionSchema.const]
        : [];
  const choices = Object.keys(binding.outcomes).filter((key) =>
    supported.some((value) => value === key),
  );
  const [choice, setChoice] = useState(choices[0] ?? '');
  const [reason, setReason] = useState('');
  const selected = choices.find((key) => key === choice);
  const eligible =
    (binding.operation === 'escalation' || acceptedCandidate) &&
    selected !== undefined &&
    reason.trim().length > 0 &&
    new TextEncoder().encode(reason.trim()).length <= reasonMaximum;
  return (
    <section>
      <SelectMenu
        ariaLabel={t('factory.operatorDisposition')}
        value={choice}
        onValueChange={setChoice}
        options={choices.map((value) => ({ value, label: value }))}
      />
      <label>
        {t('factory.operatorReason')}
        <textarea
          value={reason}
          maxLength={reasonMaximum}
          autoComplete="off"
          onChange={(event) => setReason(event.currentTarget.value)}
        />
      </label>
      {binding.operation !== 'escalation' && !acceptedCandidate ? (
        <p>{t('factory.acceptedCandidateUnavailable')}</p>
      ) : null}
      <button
        type="button"
        disabled={!eligible}
        onClick={(event) => {
          if (!eligible || !selected) return;
          const reasonValue = reason.trim();
          if (binding.operation === 'escalation') {
            const selectedDisposition = supported.find((value) => value === selected);
            if (selectedDisposition && selectedDisposition !== 'confirm')
              onReview(
                {
                  operation: binding.operation,
                  disposition: selectedDisposition,
                  reason: reasonValue,
                },
                event.currentTarget,
              );
          } else if (selected === 'confirm')
            onReview(
              { operation: binding.operation, disposition: 'confirm', reason: reasonValue },
              event.currentTarget,
            );
        }}
      >
        {t('factory.reviewOperatorRequest', { operation: binding.operation })}
      </button>
    </section>
  );
}

function HumanGateResponse({
  node,
  configuration,
  onReview,
}: {
  node: components['schemas']['FactoryFlowNodeDefinition'];
  configuration: FactoryConfigurationDetails;
  onReview: (
    body: components['schemas']['FactoryNodeResponseRequest'],
    target: HTMLElement,
  ) => void;
}) {
  const { t } = usePresentation();
  const [outcomeKey, setOutcomeKey] = useState(node.outcomes[0]?.key ?? '');
  const outcome = node.outcomes.find((outcome) => outcome.key === outcomeKey);
  const schema = configuration.configuration.flow.data_schemas.find(
    (schema) =>
      schema.reference.identity === outcome?.schema.identity &&
      schema.reference.version === outcome.schema.version &&
      schema.reference.digest === outcome.schema.digest,
  );
  const [value, setValue] = useState<unknown>(() =>
    schema ? initialFactoryValue(schema.shape) : undefined,
  );
  return (
    <section>
      <SelectMenu
        ariaLabel={t('factory.humanOutcome')}
        value={outcomeKey}
        onValueChange={(key) => {
          setOutcomeKey(key);
          const outcome = node.outcomes.find((outcome) => outcome.key === key);
          const schema = configuration.configuration.flow.data_schemas.find(
            (schema) =>
              schema.reference.identity === outcome?.schema.identity &&
              schema.reference.version === outcome.schema.version &&
              schema.reference.digest === outcome.schema.digest,
          );
          setValue(schema ? initialFactoryValue(schema.shape) : undefined);
        }}
        options={node.outcomes.map((outcome) => ({ value: outcome.key, label: outcome.key }))}
      />
      {schema ? (
        <FactoryValueInput
          schema={schema.shape}
          value={value}
          onChange={setValue}
          label={t('factory.humanResponse')}
        />
      ) : (
        <p>{t('factory.unavailableReference')}</p>
      )}
      <button
        type="button"
        disabled={!outcome || !schema || !validFactoryValue(schema.shape, value)}
        onClick={(event) => {
          if (outcome && schema && validFactoryValue(schema.shape, value))
            onReview({ outcome: outcome.key, value: structuredClone(value) }, event.currentTarget);
        }}
      >
        {t('factory.reviewHumanResponse')}
      </button>
    </section>
  );
}
