import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import type { FlowAuthoringServices, PrimitiveDraftProps } from './FlowPrimitives';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Contract = Schema['FactoryAuthoringCapabilities']['human_gates'][number];
const purposes = {
  escalation: 'factory.humanReview',
  delivery: 'factory.humanDelivery',
  promotion: 'factory.humanPromotion',
} satisfies Record<Contract['operation'], MessageKey>;
const responses = {
  acknowledge: 'factory.humanAcknowledge',
  resume: 'factory.humanResume',
  abort: 'factory.humanAbort',
  confirm: 'factory.humanConfirm',
} satisfies Record<Contract['responses'][number]['disposition'], MessageKey>;

/** Only installed management responses are offered. Result digests come from schema normalization. */
export function HumanDecision({
  node,
  contracts,
  services,
  onChange,
  draft,
  onDraftChange,
}: {
  node: Node;
  contracts: Contract[];
  services: FlowAuthoringServices;
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
} & PrimitiveDraftProps) {
  const { t } = usePresentation();
  const operation = draft?.human?.operation ?? node.operator_requests?.operation ?? '';
  const outcomes: Record<string, string> =
    draft?.human?.outcomes ?? node.operator_requests?.outcomes ?? {};
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), [node]);
  const contract = contracts.find((item) => item.operation === operation);
  const selected = contract?.responses.filter((item) => !!outcomes[item.disposition]) ?? [];
  const valid =
    selected.length > 0 &&
    selected.every((item) =>
      node.outcomes.some((outcome) => outcome.key === outcomes[item.disposition]),
    ) &&
    new Set(selected.map((item) => outcomes[item.disposition])).size === selected.length;
  const cancel = () => {
    requestRef.current?.abort();
    setPending(false);
    setError(null);
  };
  const apply = async () => {
    if (!valid || !contract || pending) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    setError(null);
    try {
      const normalized = await Promise.all(
        selected.map((response) =>
          services.api.normalizeSchema(
            {
              project_id: services.projectId,
              identity: `human.${contract.operation}.${response.disposition}`,
              version: 'v1',
              shape: response.shape,
            },
            controller.signal,
          ),
        ),
      );
      if (controller.signal.aborted) return;
      const mapping = Object.fromEntries(
        selected.map((response) => [response.disposition, outcomes[response.disposition]!]),
      );
      const binding: Schema['FactoryFlowHumanGateBinding'] | null =
        contract.operation === 'escalation'
          ? { operation: 'escalation', outcomes: mapping }
          : mapping.confirm
            ? { operation: contract.operation, outcomes: { confirm: mapping.confirm } }
            : null;
      if (!binding) return;
      onDraftChange(undefined);
      onChange(
        {
          operator_requests: binding,
          outcomes: node.outcomes.map((outcome) => {
            const index = selected.findIndex(
              (response) => outcomes[response.disposition] === outcome.key,
            );
            return index < 0 ? outcome : { ...outcome, schema: normalized[index]!.reference };
          }),
        },
        normalized,
      );
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure instanceof Error ? failure : new Error());
    } finally {
      if (requestRef.current === controller) setPending(false);
    }
  };
  return (
    <fieldset>
      <legend>{t('factory.humanDecision')}</legend>
      <p>{t('factory.humanDecisionHelp')}</p>
      <SelectMenu
        ariaLabel={t('factory.humanPurpose')}
        value={operation}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...contracts.map((item) => ({
            value: item.operation,
            label: t(purposes[item.operation]),
          })),
        ]}
        onValueChange={(value) => {
          cancel();
          onDraftChange({ human: { operation: value, outcomes: {} } });
        }}
      />
      {contract?.responses.map((response) => (
        <SelectMenu
          key={response.disposition}
          ariaLabel={t('factory.humanResponseOutcome', {
            response: t(responses[response.disposition]),
          })}
          value={outcomes[response.disposition] ?? ''}
          options={[
            { value: '', label: t('factory.humanResponseDisabled') },
            ...node.outcomes.map((outcome) => ({ value: outcome.key, label: outcome.key })),
          ]}
          onValueChange={(value) => {
            cancel();
            onDraftChange({
              human: { operation, outcomes: { ...outcomes, [response.disposition]: value } },
            });
          }}
        />
      ))}
      {contract && !node.outcomes.length ? (
        <p role="alert">{t('factory.humanNeedsOutcomes')}</p>
      ) : null}
      {selected.length > 0 && !valid ? (
        <p role="alert">{t('factory.humanDistinctOutcomes')}</p>
      ) : null}
      {node.operator_requests ? <p>{t('factory.humanConfigured')}</p> : null}
      <button type="button" disabled={!valid || pending} onClick={() => void apply()}>
        {t('factory.useHumanDecision')}
      </button>
      {pending ? <p role="status">{t('factory.checkingCondition')}</p> : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.humanDecision')}
          error={error}
          onRetry={() => void apply()}
        />
      ) : null}
    </fieldset>
  );
}
