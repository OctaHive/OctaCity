import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { ScalarEditor } from './ScalarEditor';

type Predicate = components['schemas']['FactoryFlowPredicate'];
type Operand = components['schemas']['FactoryFlowOperand'];
const seed = (): Predicate => ({ operation: 'exists', operand: { source: 'path', path: '' } });
const operations = FACTORY_AUTHORING_SCHEMAS.FactoryFlowPredicate.oneOf.map(
  (item) => item.properties.operation.const,
);
const comparisons =
  FACTORY_AUTHORING_SCHEMAS.FactoryFlowPredicate.oneOf[1].properties.comparison.enum;
/** A typed AST editor only; predicates are evaluated by the existing Factory owner. */
export function PredicateEditor({
  value,
  onChange,
  depth = 0,
}: {
  value: Predicate;
  onChange: (value: Predicate) => void;
  depth?: number;
}) {
  const { t } = usePresentation();
  const children = value.operation === 'all' || value.operation === 'any' ? value.predicates : [];
  const keys = useDraftRowKeys(children);
  const bounded = depth >= FACTORY_AUTHORING_SCHEMAS.FactoryFlowPredicate['x-max-depth'] - 1;
  return (
    <fieldset>
      <legend>{t('factory.typedCriterion')}</legend>
      <SelectMenu
        ariaLabel={t('factory.predicateOperation')}
        value={value.operation}
        options={operations
          .filter((operation) => !bounded || ['absent', 'compare', 'exists'].includes(operation))
          .map((operation) => ({ value: operation, label: operation }))}
        onValueChange={(operation) => {
          switch (operation) {
            case 'exists':
              onChange(seed());
              break;
            case 'absent':
              onChange({ operation, path: '' });
              break;
            case 'compare':
              onChange({
                operation,
                comparison: 'equal',
                left: { source: 'path', path: '' },
                right: { source: 'literal', value: '' },
              });
              break;
            case 'all':
            case 'any':
              onChange({ operation, predicates: [seed()] });
              break;
            case 'not':
              onChange({ operation, predicate: seed() });
              break;
            case 'for_each':
              onChange({
                operation,
                quantifier: 'any',
                collection: { source: 'path', path: '' },
                predicate: seed(),
              });
              break;
          }
        }}
      />
      {value.operation === 'absent' ? (
        <label>
          {t('factory.observationPath')}
          <input
            value={value.path}
            onChange={(event) => onChange({ ...value, path: event.target.value })}
          />
        </label>
      ) : null}
      {value.operation === 'exists' ? (
        <OperandEditor
          value={value.operand}
          onChange={(operand) => onChange({ ...value, operand })}
        />
      ) : null}
      {value.operation === 'compare' ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.comparison')}
            value={value.comparison}
            options={comparisons.map((comparison) => ({ value: comparison, label: comparison }))}
            onValueChange={(comparison) => onChange({ ...value, comparison })}
          />
          <OperandEditor value={value.left} onChange={(left) => onChange({ ...value, left })} />
          <OperandEditor value={value.right} onChange={(right) => onChange({ ...value, right })} />
        </>
      ) : null}
      {value.operation === 'all' || value.operation === 'any' ? (
        <>
          {value.predicates.map((predicate, index) => (
            <section key={keys[index]}>
              <PredicateEditor
                value={predicate}
                depth={depth + 1}
                onChange={(next) =>
                  onChange({
                    ...value,
                    predicates: children.map((item, at) => (at === index ? next : item)),
                  })
                }
              />
              <button
                type="button"
                onClick={() =>
                  onChange({ ...value, predicates: children.filter((_, at) => at !== index) })
                }
              >
                {t('factory.removeCriterion')}
              </button>
            </section>
          ))}
          <button
            type="button"
            disabled={bounded}
            onClick={() => onChange({ ...value, predicates: [...children, seed()] })}
          >
            {t('factory.addCriterion')}
          </button>
        </>
      ) : null}
      {value.operation === 'not' || value.operation === 'for_each' ? (
        <PredicateEditor
          value={value.predicate}
          depth={depth + 1}
          onChange={(predicate) => onChange({ ...value, predicate })}
        />
      ) : null}
      {value.operation === 'for_each' ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.quantifier')}
            value={value.quantifier}
            options={['any', 'every'].map((value) => ({ value, label: value }))}
            onValueChange={(quantifier) => {
              if (quantifier === 'any' || quantifier === 'every')
                onChange({ ...value, quantifier });
            }}
          />
          <OperandEditor
            value={value.collection}
            onChange={(collection) => onChange({ ...value, collection })}
          />
        </>
      ) : null}
    </fieldset>
  );
}
function OperandEditor({
  value,
  onChange,
}: {
  value: Operand;
  onChange: (value: Operand) => void;
}) {
  const { t } = usePresentation();
  return (
    <fieldset>
      <SelectMenu
        ariaLabel={t('factory.operandSource')}
        value={value.source}
        options={['path', 'literal', 'item', 'observation_time'].map((source) => ({
          value: source,
          label: source,
        }))}
        onValueChange={(source) => {
          if (source === 'path' || source === 'item') onChange({ source, path: '' });
          if (source === 'literal') onChange({ source, value: '' });
          if (source === 'observation_time') onChange({ source });
        }}
      />
      {'path' in value ? (
        <label>
          {t('factory.observationPath')}
          <input
            value={value.path}
            onChange={(event) => onChange({ ...value, path: event.target.value })}
          />
        </label>
      ) : null}
      {value.source === 'literal' ? (
        <ScalarEditor
          value={value.value}
          onChange={(next) => onChange({ source: 'literal', value: next })}
          typeLabel={t('factory.constantType')}
          valueLabel={t('factory.constantValue')}
        />
      ) : null}
    </fieldset>
  );
}
