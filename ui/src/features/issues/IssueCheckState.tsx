import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
/** Source-neutral labels preserve missing/skipped/unknown and unknown required policy. */
export function IssueCheckState({
  state,
  required,
}: {
  state: components['schemas']['IssueCheckState'];
  required: boolean | null;
}) {
  const { t } = usePresentation();
  return (
    <p>
      <span>{t(`issueResults.state.${state}`)}</span> ·{' '}
      <span>
        {t(
          required === null
            ? 'issueResults.requiredUnknown'
            : required
              ? 'issueResults.required'
              : 'issueResults.optional',
        )}
      </span>
    </p>
  );
}
