import { useState } from 'react';
import type { components } from '../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../.generated/api/factory-authoring';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { SelectMenu } from './SelectMenu';
import { FactoryValueInput, initialFactoryValue, validFactoryValue } from './FactoryValueInput';

export function OperatorRequest({
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

export function HumanGateResponse({
  outcomes,
  schemas,
  onReview,
}: {
  outcomes: readonly components['schemas']['FactoryFlowOutcome'][];
  schemas: readonly components['schemas']['FactoryFlowDataSchema'][];
  onReview: (
    body: components['schemas']['FactoryNodeResponseRequest'],
    target: HTMLElement,
  ) => void;
}) {
  const { t } = usePresentation();
  const [outcomeKey, setOutcomeKey] = useState(outcomes[0]?.key ?? '');
  const outcome = outcomes.find((outcome) => outcome.key === outcomeKey);
  const schema = schemas.find(
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
          const outcome = outcomes.find((outcome) => outcome.key === key);
          const schema = schemas.find(
            (schema) =>
              schema.reference.identity === outcome?.schema.identity &&
              schema.reference.version === outcome.schema.version &&
              schema.reference.digest === outcome.schema.digest,
          );
          setValue(schema ? initialFactoryValue(schema.shape) : undefined);
        }}
        options={outcomes.map((outcome) => ({ value: outcome.key, label: outcome.key }))}
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
