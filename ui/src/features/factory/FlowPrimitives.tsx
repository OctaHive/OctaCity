import { useEffect, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { FactoryApi } from './api';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { isFactoryGateProgram } from './validateDraft';
import { CommandStep } from './CommandStep';
import { HumanDecision } from './HumanDecision';
import { ValueMappingEditor } from './ValueMappingEditor';
import type { SourceUpdateDraft } from './FlowSourceUpdate';
import { PredicateEditor } from './PredicateEditor';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Program = Schema['FactoryDeclarativeGateParameters'];
export type PrimitiveDraft = {
  reporter?: SourceUpdateDraft;
  condition?: Program;
  human?: { operation: string; outcomes: Record<string, string> };
};
export type PrimitiveDraftProps = {
  draft?: PrimitiveDraft | undefined;
  onDraftChange: (draft: PrimitiveDraft | undefined) => void;
};
export type FlowAuthoringServices = { api: FactoryApi; projectId: string };

/** Installed common primitives use the same contracts as headless authors; names convey no authority. */
export function FlowPrimitives({
  node,
  services,
  onChange,
  draft,
  onDraftChange,
}: {
  node: Node;
  services: FlowAuthoringServices;
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
} & PrimitiveDraftProps) {
  const { t } = usePresentation();
  const capabilities = useQuery({
    queryKey: queryKeys.factoryAuthoringCapabilities(services.projectId),
    queryFn: ({ signal }) => services.api.getAuthoringCapabilities(services.projectId, signal),
  });
  if (capabilities.error)
    return (
      <QueryFailureNotice
        title={t('factory.stepCapabilities')}
        error={capabilities.error}
        onRetry={() => void capabilities.refetch()}
      />
    );
  if (!capabilities.data) return <p role="status">{t('factory.loadingStepCapabilities')}</p>;
  return node.kind === 'deterministic_gate' ? (
    <ConditionEditor
      draft={draft}
      onDraftChange={onDraftChange}
      node={node}
      services={services}
      contract={capabilities.data.deterministic_gate}
      onChange={onChange}
    />
  ) : node.kind === 'human_gate' ? (
    <HumanDecision
      draft={draft}
      onDraftChange={onDraftChange}
      node={node}
      services={services}
      contracts={capabilities.data.human_gates}
      onChange={onChange}
    />
  ) : node.kind === 'build_command' ? (
    <CommandStep node={node} commands={capabilities.data.commands ?? []} onChange={onChange} />
  ) : null;
}

function ConditionEditor({
  node,
  services,
  contract,
  onChange,
  draft,
  onDraftChange,
}: {
  node: Node;
  services: FlowAuthoringServices;
  contract: Schema['FactoryAuthoringCapabilities']['deterministic_gate'];
  onChange: (patch: Partial<Node>) => void;
} & PrimitiveDraftProps) {
  const { t } = usePresentation();
  const program =
    draft?.condition ??
    (isFactoryGateProgram(node.gate?.parameters)
      ? node.gate.parameters
      : { rules: [], fallback: '' });
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), [node]);
  const ruleKeys = useDraftRowKeys(program.rules);
  const outcomes = node.outcomes.map((item) => ({ value: item.key, label: item.key }));
  const edit = (next: Program) => {
    requestRef.current?.abort();
    setPending(false);
    setError(null);
    onDraftChange({ condition: next });
  };
  const valid =
    outcomes.some((item) => item.value === program.fallback) &&
    program.rules.every((rule) => outcomes.some((item) => item.value === rule.outcome)) &&
    new TextEncoder().encode(JSON.stringify(program)).length <= contract.max_parameter_bytes;
  const apply = async () => {
    if (!valid || pending) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    setError(null);
    try {
      const gate = await services.api.normalizeGateProgram(
        services.projectId,
        { program },
        controller.signal,
      );
      if (!controller.signal.aborted) {
        onDraftChange(undefined);
        onChange({ gate, build: null });
      }
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure instanceof Error ? failure : new Error());
    } finally {
      if (requestRef.current === controller) setPending(false);
    }
  };
  return (
    <fieldset>
      <legend>{t('factory.conditionCheck')}</legend>
      <p>{t('factory.conditionHelp')}</p>
      <SelectMenu
        ariaLabel={t('factory.conditionFallback')}
        value={program.fallback}
        options={[{ value: '', label: t('factory.chooseInput') }, ...outcomes]}
        onValueChange={(fallback) => edit({ ...program, fallback })}
      />
      {program.rules.map((rule, index) => (
        <fieldset key={ruleKeys[index]}>
          <legend>{t('factory.conditionNumber', { number: index + 1 })}</legend>
          <PredicateEditor
            value={rule.when}
            onChange={(when) =>
              edit({
                ...program,
                rules: program.rules.map((item, at) => (at === index ? { ...item, when } : item)),
              })
            }
          />
          <SelectMenu
            ariaLabel={t('factory.conditionOutcome')}
            value={rule.outcome}
            options={[{ value: '', label: t('factory.chooseInput') }, ...outcomes]}
            onValueChange={(outcome) =>
              edit({
                ...program,
                rules: program.rules.map((item, at) =>
                  at === index ? { ...item, outcome } : item,
                ),
              })
            }
          />
          <button
            type="button"
            onClick={() =>
              edit({ ...program, rules: program.rules.filter((_, at) => at !== index) })
            }
          >
            {t('factory.removeCondition')}
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={
          program.rules.length >=
          FACTORY_AUTHORING_SCHEMAS.FactoryDeclarativeGateParameters.properties.rules.maxItems
        }
        onClick={() =>
          edit({
            ...program,
            rules: [
              ...program.rules,
              { outcome: '', when: { operation: 'exists', operand: { source: 'path', path: '' } } },
            ],
          })
        }
      >
        {t('factory.addCondition')}
      </button>
      <label>
        <input
          type="checkbox"
          checked={program.default_output != null}
          onChange={(event) => {
            if (event.target.checked)
              edit({ ...program, default_output: { value: { mapping: 'pointer', path: '' } } });
            else {
              const { default_output: _removed, ...rest } = program;
              void _removed;
              edit(rest);
            }
          }}
        />
        {t('factory.chooseConditionData')}
      </label>
      {program.default_output ? (
        <fieldset>
          <legend>{t('factory.defaultConditionData')}</legend>
          <p>{t('factory.conditionDataHelp')}</p>
          <ValueMappingEditor
            value={program.default_output.value}
            onChange={(value) => edit({ ...program, default_output: { value } })}
          />
        </fieldset>
      ) : null}
      {node.gate ? <p>{t('factory.conditionConfigured')}</p> : null}
      <button type="button" disabled={!valid || pending} onClick={() => void apply()}>
        {t('factory.useCondition')}
      </button>
      {pending ? <p role="status">{t('factory.checkingCondition')}</p> : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.conditionCheck')}
          error={error}
          onRetry={() => void apply()}
        />
      ) : null}
    </fieldset>
  );
}
