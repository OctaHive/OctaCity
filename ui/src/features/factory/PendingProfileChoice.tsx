import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { useDeadline } from '../../shared/useDeadline';
import { SafeFactoryMetadata } from '../../shared/FactoryDiagnosticRecord';
import { SelectMenu } from '../../shared/SelectMenu';

type Projection = components['schemas']['FactoryProfileSelectionProjection'];
/** An offer is the server-projected finite set, scoped to this exact input and original deadline. */
export function PendingProfileChoice({
  selection,
  disabled,
  onReview,
}: {
  selection: Projection;
  disabled: boolean;
  onReview: (
    profile: components['schemas']['FactoryRoleProfileReference'],
    target: HTMLElement,
  ) => void;
}) {
  const { t, locale } = usePresentation();
  const expired = useDeadline(selection.deadline);
  const [candidate, setCandidate] = useState('');
  if (
    selection.superseded ||
    selection.profile ||
    selection.pending_control ||
    selection.control_id ||
    expired ||
    !selection.eligible_candidates.length
  )
    return null;
  const selected =
    selection.eligible_candidates.find((item) => item.candidate === candidate) ??
    selection.eligible_candidates[0]!;
  return (
    <fieldset disabled={disabled}>
      <legend>{t('factory.pendingProfile', { node: selection.node })}</legend>
      <p>
        {t('factory.profileDeadline', {
          deadline: new Date(selection.deadline).toLocaleString(locale),
        })}
      </p>
      <SafeFactoryMetadata
        value={selection}
        fields={['input_digest', 'flow_run_id', 'cycle_id', 'selector_error', 'recovery']}
      />
      <SelectMenu
        ariaLabel={t('factory.manualProfile')}
        value={selected.candidate}
        options={selection.eligible_candidates.map((item) => ({
          value: item.candidate,
          label: `${item.candidate} · ${item.profile.id} v${item.profile.version}`,
        }))}
        onValueChange={setCandidate}
      />
      <button
        type="button"
        onClick={(event) => {
          if (Date.now() < selection.deadline) onReview(selected.profile, event.currentTarget);
        }}
      >
        {t('factory.reviewProfileChoice')}
      </button>
    </fieldset>
  );
}
