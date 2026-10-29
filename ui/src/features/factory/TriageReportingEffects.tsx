import { useState } from 'react';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { FiniteSourceEffect } from '../../shared/SourceReporting';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import {
  TriageReportingPolicy,
  initialReportingPolicy,
  type ReportingPolicy,
} from './TriageReportingPolicy';
import type { TriageSourceDraft } from './TriageSourceSelection';
import { TriageReportingDimensions } from './TriageReportingDimensions';

type Schema = components['schemas'];
type Effects = NonNullable<Schema['FactoryPresetResolutionRequest']['effects']>;
type ReportingSelection = ReportingPolicy &
  Pick<Schema['FactoryResultReportingDraft'], 'outcomes' | 'dimensions'>;
interface Props {
  flow: Schema['FactoryFlowDefinitionDraft'];
  selection: TriageSourceDraft;
  value: Effects;
  onChange: (value: Effects) => void;
}
/** Finite effects reuse the registered producer and installed literal choices, without prior Flow history. */
export function TriageReportingEffects({ flow, selection, value, onChange }: Props) {
  const { t } = usePresentation();
  const policy = selection.writer?.reporting?.editable;
  const offered = selection.writer?.reporting?.action;
  const contract =
    offered &&
    selection.input.reporter &&
    offered.action === 'set_status' &&
    sourceReferenceKey(offered.plugin) ===
      sourceReferenceKey(selection.input.reporter.capability) &&
    sourceReferenceKey(offered.output.reference) === sourceReferenceKey(offered.outcome.schema)
      ? offered
      : null;
  return (
    <section aria-label={t('triage.effects')}>
      <h3>{t('triage.effects')}</h3>
      <p>{t('triage.labelScopeHelp')}</p>
      <Link
        to={`/integrations/${encodeURIComponent(selection.input.connection.id)}`}
        target="_blank"
        rel="noreferrer"
      >
        {t('triage.labelScope')}
      </Link>
      {!contract || !policy ? (
        <p>{t('triage.reportingContractMissing')}</p>
      ) : (
        <p>{t('triage.nativeReportingContract')}</p>
      )}
      {Object.keys(value).map((gate) => (
        <button
          key={gate}
          type="button"
          onClick={() =>
            onChange(Object.fromEntries(Object.entries(value).filter(([key]) => key !== gate)))
          }
        >
          {t('triage.removeEffects', { gate })}
        </button>
      ))}
      {contract && policy && selection.input.reporter
        ? flow.root.nodes
            .filter((node) => node.kind === 'deterministic_gate')
            .map((gate) => (
              <GateEffects
                key={gate.key}
                gate={gate}
                selected={value[gate.key]}
                policy={policy}
                ceiling={flow.root.execution}
                schemas={flow.data_schemas}
                onApply={(reviewed) =>
                  onChange({
                    ...value,
                    [gate.key]: {
                      id: value[gate.key]?.id ?? crypto.randomUUID(),
                      definition: {
                        target: {
                          source: selection.input.source,
                          reporter: selection.input.reporter!,
                          connection: selection.input.connection,
                          policy,
                        },
                        ...reviewed,
                        receipt: contract.output,
                        action_outcome: contract.outcome.key,
                      },
                    },
                  })
                }
              />
            ))
        : null}
    </section>
  );
}
function GateEffects({
  gate,
  selected,
  policy,
  ceiling,
  schemas,
  onApply,
}: {
  gate: Schema['FactoryFlowNodeDefinition'];
  selected: Effects[string] | undefined;
  policy: Schema['WorkSourceReportingPolicy'];
  ceiling: Schema['FactoryFlowExecutionPolicy'];
  schemas: Schema['FactoryFlowDataSchema'][];
  onApply: (reviewed: ReportingSelection) => void;
}) {
  const { t } = usePresentation();
  const [outcomes, setOutcomes] = useState<Schema['FactoryResultReportingDraft']['outcomes']>(
    () =>
      selected?.definition.outcomes ??
      Object.fromEntries(gate.outcomes.map((outcome) => [outcome.key, null])),
  );
  const [parameters, setParameters] = useState<ReportingPolicy>(() =>
    selected
      ? {
          budget: selected.definition.budget,
          execution: selected.definition.execution,
          retry: selected.definition.retry,
        }
      : initialReportingPolicy(),
  );
  const [dimensions, setDimensions] = useState<Schema['FactoryReportingDimension'][]>(
    () => selected?.definition.dimensions ?? [],
  );
  const result = gate.outcomes.every(
    (outcome) =>
      sourceReferenceKey(outcome.schema) === sourceReferenceKey(gate.outcomes[0]!.schema),
  )
    ? schemas.find(
        (schema) =>
          sourceReferenceKey(schema.reference) === sourceReferenceKey(gate.outcomes[0]!.schema),
      )
    : undefined;
  const valid = [
    ...Object.values(outcomes),
    ...dimensions.flatMap((dimension) => dimension.cases.map((row) => row.effects)),
  ].every(
    (effect) =>
      !effect ||
      (effect.add.every(
        (label) => policy.labels.includes(label) && !effect.remove.includes(label),
      ) &&
        effect.remove.every((label) => policy.labels.includes(label)) &&
        (effect.state === null || policy.states.includes(effect.state))),
  );
  return (
    <section>
      <h4>{gate.key}</h4>
      <TriageReportingPolicy value={parameters} ceiling={ceiling} onChange={setParameters} />
      {!valid ? <p role="alert">{t('triage.labelScopeHelp')}</p> : null}
      {gate.outcomes.map(({ key }) => {
        const effect = outcomes[key];
        return (
          <fieldset key={key}>
            <legend>{key}</legend>
            <FiniteSourceEffect
              label={t('triage.reportOutcome')}
              status={key}
              value={effect ?? null}
              policy={policy}
              onChange={(next) => setOutcomes({ ...outcomes, [key]: next })}
            />
          </fieldset>
        );
      })}
      <TriageReportingDimensions
        shape={result?.shape}
        outcomes={gate.outcomes.map((outcome) => outcome.key)}
        value={dimensions}
        policy={policy}
        onChange={setDimensions}
      />
      <button
        type="button"
        disabled={!valid}
        onClick={() => onApply({ ...parameters, outcomes, dimensions })}
      >
        {t('triage.useEffects')}
      </button>
    </section>
  );
}
