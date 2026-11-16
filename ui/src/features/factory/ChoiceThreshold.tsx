import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type S = components['schemas'];
type Predicate = S['FactoryFlowPredicate'];
type Choice = { path: string; label: string; values: string[] };
function choiceFields(shape: S['FactoryFlowValueSchema'], parts: string[] = []): Choice[] {
  if (shape.type === 'object')
    return Object.entries(shape.fields).flatMap(([name, field]) =>
      choiceFields(field.value, [...parts, name]),
    );
  if (shape.type !== 'enum' || !shape.values.every((value) => typeof value === 'string')) return [];
  return [
    {
      path: parts.map((part) => `/${part.replaceAll('~', '~0').replaceAll('/', '~1')}`).join(''),
      label: parts.join(' › '),
      values: shape.values.filter((value): value is string => typeof value === 'string'),
    },
  ];
}
const comparison = (path: string, value: string): Predicate => ({
  operation: 'compare',
  comparison: 'equal',
  left: { source: 'path', path },
  right: { source: 'literal', value },
});
function pathOf(value: Predicate): string | null {
  const first = value.operation === 'any' ? value.predicates[0] : value;
  return first?.operation === 'compare' && first.left.source === 'path' ? first.left.path : null;
}

/** Compile an inclusive ordered choice range into the ordinary editable predicate grammar. */
export function ChoiceThreshold({
  shape,
  value,
  condition,
  onChange,
}: {
  shape: S['FactoryFlowValueSchema'];
  value: Predicate;
  condition: string;
  onChange: (value: Predicate) => void;
}) {
  const { t } = usePresentation();
  const [path, setPath] = useState<string | null>(null);
  const fields = choiceFields(shape);
  const selected = fields.find((field) => field.path === (path ?? pathOf(value)));
  const equal = (predicate: Predicate | undefined, path: string, choice: string) =>
    predicate?.operation === 'compare' &&
    predicate.comparison === 'equal' &&
    predicate.left.source === 'path' &&
    predicate.left.path === path &&
    predicate.right.source === 'literal' &&
    predicate.right.value === choice;
  const threshold = selected?.values.find((_, index) => {
    const suffix = selected.values.slice(index);
    return value.operation === 'any'
      ? value.predicates.length === suffix.length &&
          suffix.every((choice, at) => equal(value.predicates[at], selected.path, choice))
      : suffix.length === 1 && equal(value, selected.path, suffix[0]!);
  });
  if (!fields.length) return null;
  return (
    <>
      <SelectMenu
        ariaLabel={t('factory.choiceField')}
        value={selected?.path ?? ''}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...fields.map((field) => ({ value: field.path, label: field.label })),
        ]}
        onValueChange={setPath}
      />
      {selected ? (
        <SelectMenu
          ariaLabel={t('factory.choiceThreshold', { field: selected.label, condition })}
          value={threshold ?? ''}
          options={[
            { value: '', label: t('factory.chooseInput') },
            ...selected.values.map((value) => ({ value, label: value })),
          ]}
          onValueChange={(value) => {
            const start = selected.values.indexOf(value);
            if (start >= 0)
              onChange({
                operation: 'any',
                predicates: selected.values
                  .slice(start)
                  .map((value) => comparison(selected.path, value)),
              });
          }}
        />
      ) : null}
      <p>{t('factory.inputChoiceThresholdHelp')}</p>
    </>
  );
}
