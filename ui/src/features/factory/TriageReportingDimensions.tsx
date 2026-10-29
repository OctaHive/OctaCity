import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_SETUP_SCHEMAS } from '../../../.generated/api/factory-setup';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { FiniteSourceEffect } from '../../shared/SourceReporting';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';

type Schema = components['schemas'];
type Dimension = Schema['FactoryReportingDimension'];
type Field = {
  path: string[];
  shape: Extract<Schema['FactoryFlowValueSchema'], { type: 'enum' | 'integer' }>;
};
const maximumCases = FACTORY_SETUP_SCHEMAS.FactoryReportingDimension.properties.cases.maxItems;
/** Only required finite fields from the protected result contract are selectable. */
function finiteFields(shape: Schema['FactoryFlowValueSchema'], path: string[] = []): Field[] {
  if (
    path.length &&
    shape.type === 'enum' &&
    shape.values.length <= maximumCases &&
    shape.values.every((value) => typeof value !== 'number' || Number.isSafeInteger(value))
  )
    return [{ path, shape }];
  if (
    path.length &&
    shape.type === 'integer' &&
    Number.isSafeInteger(shape.minimum) &&
    Number.isSafeInteger(shape.maximum)
  )
    return [{ path, shape }];
  return shape.type === 'object'
    ? Object.entries(shape.fields)
        .filter(([, field]) => field.required)
        .flatMap(([name, field]) => finiteFields(field.value, [...path, name]))
    : [];
}
const fieldLabel = (path: readonly string[]) => path.join(' / ');
const caseLabel = (selection: Schema['FactoryReportingSelection']) =>
  selection.selection === 'literal'
    ? String(selection.value)
    : `${selection.minimum}–${selection.maximum}`;

/** Each selected field is one independent bounded dimension; this editor never constructs combinations. */
export function TriageReportingDimensions({
  shape,
  outcomes,
  value,
  policy,
  onChange,
}: {
  shape: Schema['FactoryFlowValueSchema'] | undefined;
  outcomes: readonly string[];
  value: Dimension[];
  policy: Schema['WorkSourceReportingPolicy'];
  onChange: (value: Dimension[]) => void;
}) {
  const { t } = usePresentation();
  const fields = shape ? finiteFields(shape) : [];
  const [selected, setSelected] = useState('');
  const [selectedOutcomes, setSelectedOutcomes] = useState<string[]>([]);
  const field = fields.find((item) => JSON.stringify(item.path) === selected);
  const canAdd =
    !!field &&
    selectedOutcomes.length > 0 &&
    value.length <
      FACTORY_SETUP_SCHEMAS.FactoryResultReportingDraft.properties.dimensions.maxItems &&
    !value.some((dimension) => JSON.stringify(dimension.path) === selected);
  function change(index: number, next: Dimension) {
    onChange(value.map((dimension, i) => (i === index ? next : dimension)));
  }
  return (
    <fieldset>
      <legend>{t('triage.dimensions')}</legend>
      <p>{t('triage.dimensionsHelp')}</p>
      <label>
        <span>{t('triage.dimensionField')}</span>
        <SelectMenu
          ariaLabel={t('triage.dimensionField')}
          value={selected}
          options={[
            { value: '', label: t('factory.none') },
            ...fields.map((item) => ({
              value: JSON.stringify(item.path),
              label: fieldLabel(item.path),
            })),
          ]}
          onValueChange={setSelected}
        />
      </label>
      <fieldset>
        <legend>{t('triage.dimensionOutcomes')}</legend>
        {outcomes.map((outcome) => (
          <label key={outcome}>
            <input
              type="checkbox"
              checked={selectedOutcomes.includes(outcome)}
              onChange={(event) =>
                setSelectedOutcomes(
                  event.target.checked
                    ? [...selectedOutcomes, outcome]
                    : selectedOutcomes.filter((item) => item !== outcome),
                )
              }
            />
            {outcome}
          </label>
        ))}
      </fieldset>
      <button
        type="button"
        disabled={!canAdd}
        onClick={() => {
          if (!canAdd || !field) return;
          const cases: Dimension['cases'] =
            field.shape.type === 'enum'
              ? field.shape.values.map((item) => ({
                  selection: { selection: 'literal', value: item },
                  effects: null,
                }))
              : [
                  {
                    selection: {
                      selection: 'integer_range',
                      minimum: field.shape.minimum,
                      maximum: field.shape.maximum,
                    },
                    effects: null,
                  },
                ];
          onChange([
            ...value,
            { key: crypto.randomUUID(), path: field.path, outcomes: [...selectedOutcomes], cases },
          ]);
          setSelected('');
          setSelectedOutcomes([]);
        }}
      >
        {t('triage.addDimension')}
      </button>
      {value.map((dimension, index) => (
        <section
          key={dimension.key}
          aria-label={t('triage.dimensionTitle', { field: fieldLabel(dimension.path) })}
        >
          <h5>{fieldLabel(dimension.path)}</h5>
          <p>{dimension.outcomes.join(', ')}</p>
          <DimensionCases
            dimension={dimension}
            policy={policy}
            onChange={(next) => change(index, next)}
          />
          <button type="button" onClick={() => onChange(value.filter((_, i) => i !== index))}>
            {t('triage.removeDimension')}
          </button>
        </section>
      ))}
    </fieldset>
  );
}

function DimensionCases({
  dimension,
  policy,
  onChange,
}: {
  dimension: Dimension;
  policy: Schema['WorkSourceReportingPolicy'];
  onChange: (value: Dimension) => void;
}) {
  const { t } = usePresentation();
  const keys = useDraftRowKeys(dimension.cases);
  function change(index: number, next: Dimension['cases'][number]) {
    onChange({ ...dimension, cases: dimension.cases.map((row, i) => (i === index ? next : row)) });
  }
  return (
    <>
      {dimension.cases.map((row, index) => (
        <fieldset key={keys[index]}>
          <legend>{caseLabel(row.selection)}</legend>
          {row.selection.selection === 'integer_range' ? (
            <IntegerBand
              selection={row.selection}
              disabled={dimension.cases.length >= maximumCases}
              onSplit={(boundary) => {
                if (row.selection.selection !== 'integer_range') return;
                onChange({
                  ...dimension,
                  cases: [
                    ...dimension.cases.slice(0, index),
                    { ...row, selection: { ...row.selection, maximum: boundary } },
                    { ...row, selection: { ...row.selection, minimum: boundary + 1 } },
                    ...dimension.cases.slice(index + 1),
                  ],
                });
              }}
            />
          ) : null}
          <FiniteSourceEffect
            label={t('triage.reportChoice')}
            status={dimension.path.at(-1)!}
            displayStatus={caseLabel(row.selection)}
            value={row.effects}
            policy={policy}
            onChange={(effects) => change(index, { ...row, effects })}
          />
        </fieldset>
      ))}
    </>
  );
}
function IntegerBand({
  selection,
  disabled,
  onSplit,
}: {
  selection: Extract<Schema['FactoryReportingSelection'], { selection: 'integer_range' }>;
  disabled: boolean;
  onSplit: (boundary: number) => void;
}) {
  const { t } = usePresentation();
  const [boundary, setBoundary] = useState(selection.minimum);
  const valid =
    !disabled &&
    Number.isSafeInteger(boundary) &&
    boundary >= selection.minimum &&
    boundary < selection.maximum;
  return (
    <>
      <label>
        {t('triage.splitAt')}
        <input
          type="number"
          min={selection.minimum}
          max={selection.maximum - 1}
          value={boundary}
          onChange={(event) => setBoundary(Number(event.target.value))}
        />
      </label>
      <button
        type="button"
        disabled={!valid}
        onClick={() => {
          if (valid) onSplit(boundary);
        }}
      >
        {t('triage.splitBand')}
      </button>
    </>
  );
}
