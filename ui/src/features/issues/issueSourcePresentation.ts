import type { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueSourceMappingDefinition } from './sourceApi';
type Translate = ReturnType<typeof usePresentation>['t'];
export type SourceAuthority = IssueSourceMappingDefinition['fields'][string];
export function sourceFieldLabel(field: string, t: Translate): string {
  return field === 'title'
    ? t('issues.title')
    : field === 'description'
      ? t('issues.description')
      : field === 'status'
        ? t('issueSource.statusField')
        : field;
}
export function sourceAuthorityLabel(authority: SourceAuthority, t: Translate): string {
  return t(
    authority === 'source_owned_pull'
      ? 'issueSource.pullAuthority'
      : authority === 'octacity_owned_push'
        ? 'issueSource.pushAuthority'
        : 'issueSource.localAuthority',
  );
}
