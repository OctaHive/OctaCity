import type { usePresentation } from '../app/presentation/PresentationProvider';
import type { components } from '../../.generated/api/schema';

/** Shared normalized filter description for Factory and Project import reviews. */
export function sourcePredicateSummary(
  predicate: components['schemas']['WorkSourcePredicate'] | null | undefined,
  t: ReturnType<typeof usePresentation>['t'],
  separator = ', ',
): string {
  return predicate
    ? [
        `${t('factory.taskState')}: ${predicate.state ?? t('factory.anyState')}`,
        t(predicate.label_presence === 'none' ? 'factory.labelsNone' : 'factory.labelsAny'),
        `${t('factory.allLabels')}: ${predicate.all_of.join(separator)}`,
        `${t('factory.anyLabels')}: ${predicate.any_of.join(separator)}`,
        `${t('factory.excludedLabels')}: ${predicate.none_of.join(separator)}`,
      ].join('; ')
    : t('factory.installedFilters');
}
