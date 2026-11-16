import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';

export { classificationFields, sameIssueFieldTarget } from './issueFields';

type S = components['schemas'];
export type IssueFieldChoice = S['FactoryIssueFieldChoice'];
export type IssueFieldChoices = S['FactoryIssueFieldChoices'];
type Translate = ReturnType<typeof usePresentation>['t'];
const labels = {
  title: 'issues.title',
  description: 'issues.description',
  acceptance: 'issues.acceptance',
  priority: 'issues.priority',
  component: 'classification.component',
  type: 'classification.type',
  size: 'classification.size',
  dependencies: 'classification.dependencies',
} satisfies Record<
  Exclude<IssueFieldChoice['source']['kind'], 'property' | 'classification'>,
  MessageKey
>;

export function issueFieldLabel(
  field: Pick<IssueFieldChoice, 'source'> & Partial<Pick<IssueFieldChoice, 'name'>>,
  t: Translate,
) {
  return field.source.kind === 'property'
    ? t('factory.issuePropertyField', { name: field.name ?? field.source.id })
    : field.source.kind === 'classification'
      ? t('factory.issueClassificationField', { name: field.name ?? field.source.slot })
      : t(labels[field.source.kind]);
}

/** Readable types and bounds are advisory views of the authoritative descriptor. */
export function IssueFieldType({ field }: { field: IssueFieldChoice }) {
  const { t } = usePresentation();
  const shape = field.shape;
  const kind =
    field.value_type?.type ??
    (shape.type === 'enum'
      ? 'choice'
      : shape.type === 'integer'
        ? 'number'
        : shape.type === 'boolean'
          ? 'boolean'
          : 'text');
  return (
    <p>
      {t(`issueFields.${kind}`)}
      {shape.type === 'enum'
        ? ` · ${shape.values.map((id) => field.choice_labels?.[String(id)] ?? String(id)).join(', ')}`
        : null}
      {shape.type === 'integer' ? ` · ${shape.minimum}–${shape.maximum}` : null}
      {shape.type === 'string' && kind !== 'date'
        ? ` · ${t('issueFields.textLimit')}: ${shape.max_bytes}`
        : null}
    </p>
  );
}
