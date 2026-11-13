import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { FIELD_KIND_LABELS } from './IssueFieldDefinitions';

type Definition = components['schemas']['IssuePropertyDefinitionSet'];
/** Review freezes the complete proposed catalog, including retained keys, bounds and semantic IDs. */
export function IssueFieldCatalogReview({ definition }: { definition: Definition }) {
  const { t } = usePresentation();
  const choices = (items: components['schemas']['IssuePropertyChoice'][]) => (
    <ul>
      {items.map((item) => (
        <li key={item.id}>
          {item.name} · {item.id} · {t(item.retired ? 'issueFields.retired' : 'issueFields.active')}
        </li>
      ))}
    </ul>
  );
  return (
    <>
      {(['priorities', 'components', 'types'] as const).map((list) => (
        <section key={list}>
          <h3>{t(`issueFields.${list}`)}</h3>
          {choices(definition[list] ?? [])}
        </section>
      ))}
      <h3>{t('issueFields.custom')}</h3>
      <ul>
        {definition.properties.map((field) => (
          <li key={field.id}>
            <p>
              {field.name} · {field.id} · {t(FIELD_KIND_LABELS[field.kind.type])} ·{' '}
              {t(field.retired ? 'issueFields.retired' : 'issueFields.active')}
            </p>
            {field.kind.type === 'text' ? (
              <p>
                {t('issueFields.textLimit')}: {field.kind.max_bytes}
              </p>
            ) : null}
            {field.kind.type === 'number' ? (
              <p>
                {t('issueFields.minimum')}: {field.kind.minimum} · {t('issueFields.maximum')}:{' '}
                {field.kind.maximum}
              </p>
            ) : null}
            {field.kind.type === 'choice' ? choices(field.kind.choices) : null}
          </li>
        ))}
      </ul>
      <h3>{t('issueFields.semantics')}</h3>
      <ul>
        {Object.entries(definition.classification ?? {}).map(([slot, id]) => (
          <li key={slot}>
            {t(`issueFields.binding.${slot as keyof NonNullable<Definition['classification']>}`)} ·{' '}
            {id}
          </li>
        ))}
      </ul>
    </>
  );
}
