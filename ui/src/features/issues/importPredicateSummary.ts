import type { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueSourceImport } from './sourceApi';

/** The same retained filters appear in edit and lifecycle reviews. */
export function importPredicateSummary(
  predicate: IssueSourceImport['definition']['predicate'],
  t: ReturnType<typeof usePresentation>['t'],
): string {
  return predicate
    ? [
        `${t('factory.taskState')}: ${predicate.state ?? t('factory.anyState')}`,
        `${t('factory.allLabels')}: ${predicate.all_of.join(', ')}`,
        `${t('factory.anyLabels')}: ${predicate.any_of.join(', ')}`,
        `${t('factory.excludedLabels')}: ${predicate.none_of.join(', ')}`,
      ].join('; ')
    : t('factory.installedFilters');
}
