import type { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssuePlanning, IssuePropertyDefinition, IssuePropertyValue } from './api';

/** Human labels are resolved from the reviewed catalog, never inferred from provider IDs. */
export function propertyValueLabel(
  value: IssuePropertyValue | undefined,
  definition: IssuePropertyDefinition | undefined,
  t: ReturnType<typeof usePresentation>['t'],
): string {
  if (!value) return t('issues.notSet');
  if (value.type === 'boolean') return t(value.value ? 'planning.yes' : 'planning.no');
  if (value.type === 'choice')
    return definition?.kind.type === 'choice'
      ? (definition.kind.choices.find((c) => c.id === value.value)?.name ??
          t('planning.unavailableValue'))
      : t('planning.unavailableValue');
  return String(value.value);
}

/** Catalog keys may be valid JavaScript prototype names; only own values are task data. */
export function propertyValue(
  values: IssuePlanning['values'] | undefined,
  id: string,
): IssuePropertyValue | undefined {
  return values && Object.hasOwn(values, id) ? values[id] : undefined;
}
