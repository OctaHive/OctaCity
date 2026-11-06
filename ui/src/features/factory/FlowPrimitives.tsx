import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
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
import { PublicationStep, publishCandidateAction } from './PublicationStep';

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
  const publication = node.kind === 'trusted_action';
  const [inspectPublication, setInspectPublication] = useState(false);
  const publicationSelected = node.action?.action === publishCandidateAction;
  const canConfigurePublication = publication && (!node.action || publicationSelected);
  const capabilities = useFactoryAuthoringCapabilities(
    services,
    !publication || (canConfigurePublication && (inspectPublication || publicationSelected)),
  );
  if (publication && !canConfigurePublication) return null;
  if (publication && !inspectPublication && !publicationSelected)
    return (
      <button type="button" onClick={() => setInspectPublication(true)}>
        {t('factory.publicationAction')}
      </button>
    );
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
  ) : node.kind === 'trusted_action' ? (
    <PublicationStep
      node={node}
      choices={capabilities.data.candidate_publications ?? []}
      onChange={onChange}
    />
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
      <ConditionProjection
        label={t('factory.defaultConditionData')}
        action={t('factory.chooseConditionData')}
        help={t('factory.conditionDataHelp')}
        value={program.default_output}
        onChange={(default_output) => {
          if (default_output) edit({ ...program, default_output });
          else {
            const { default_output: _removed, ...rest } = program;
            void _removed;
            edit(rest);
          }
        }}
      />
      <details>
        <summary>{t('factory.advancedConditions')}</summary>
        <label>
          <input
            type="checkbox"
            checked={!!program.precondition}
            onChange={(event) =>
              edit(
                event.target.checked
                  ? {
                      ...program,
                      precondition: { operation: 'exists', operand: { source: 'path', path: '' } },
                    }
                  : { ...program, precondition: null, precondition_failure_output: null },
              )
            }
          />
          {t('factory.conditionPrecheck')}
        </label>
        {program.precondition ? (
          <>
            <PredicateEditor
              value={program.precondition}
              onChange={(precondition) => edit({ ...program, precondition })}
            />
            <ConditionProjection
              label={t('factory.conditionFailedPrecheckData')}
              value={program.precondition_failure_output}
              onChange={(precondition_failure_output) =>
                edit({ ...program, precondition_failure_output })
              }
            />
          </>
        ) : null}
        {outcomes.map(({ value: outcome }) => (
          <ConditionProjection
            key={outcome}
            label={t('factory.conditionOutcomeData', { outcome })}
            action={t('factory.conditionProjectOutcome', { outcome })}
            value={program.outputs?.[outcome]}
            onChange={(mapping) => {
              const outputs = { ...program.outputs };
              if (mapping) outputs[outcome] = mapping;
              else delete outputs[outcome];
              edit({ ...program, outputs });
            }}
          />
        ))}
      </details>
      {node.gate ? <p>{t('factory.conditionConfigured')}</p> : null}
      {isFactoryGateProgram(node.gate?.parameters)
        ? node.outcomes
            .filter(({ kind }) => kind === 'success')
            .map(({ key }) => {
              const accepted = node.accepted_work_outcomes ?? [];
              return (
                <label key={key}>
                  <input
                    type="checkbox"
                    checked={accepted.includes(key)}
                    onChange={(event) =>
                      onChange({
                        accepted_work_outcomes: event.target.checked
                          ? [...accepted, key].sort()
                          : accepted.filter((outcome) => outcome !== key),
                      })
                    }
                  />
                  {t('factory.acceptWorkOnOutcome', { outcome: key })}
                </label>
              );
            })
        : null}
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

function ConditionProjection({
  label,
  action = label,
  help,
  value,
  onChange,
}: {
  label: string;
  action?: string;
  help?: string;
  value: Schema['FactoryFlowDataMapping'] | null | undefined;
  onChange: (mapping: Schema['FactoryFlowDataMapping'] | null) => void;
}) {
  return (
    <>
      <label>
        <input
          type="checkbox"
          checked={!!value}
          onChange={(event) =>
            onChange(event.target.checked ? { value: { mapping: 'pointer', path: '' } } : null)
          }
        />
        {action}
      </label>
      {value ? (
        <fieldset>
          <legend>{label}</legend>
          {help ? <p>{help}</p> : null}
          <ValueMappingEditor value={value.value} onChange={(value) => onChange({ value })} />
        </fieldset>
      ) : null}
    </>
  );
}
