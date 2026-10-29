import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

type Mapping = components['schemas']['SourceAcceptanceMapping'];
/** Shows the exact installed mapping, without inferring acceptance from task text or Role names. */
export function SourcePreparationSummary({
  value,
  projectId,
}: {
  value: Mapping | undefined;
  projectId: string;
}) {
  const { t } = usePresentation();
  if (!value) return <p>{t('triage.preparationUnknown')}</p>;
  if (value.kind !== 'process_completion')
    return (
      <p>
        {t('triage.authoredAcceptance')}
        {value.kind === 'body' ? '' : ` · ${value.value}`}
      </p>
    );
  if (value.value.project_id !== projectId) return <p>{t('triage.preparationUnknown')}</p>;
  return (
    <section>
      <h3>{t('triage.completion')}</h3>
      <p>{t('triage.completionHelp')}</p>
      {value.value.kind === 'draft' ? (
        <p style={{ whiteSpace: 'pre-wrap' }}>{value.value.content}</p>
      ) : (
        <p>{t('triage.retainedDocument')}</p>
      )}
    </section>
  );
}
