import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceBindingKey } from '../../shared/sourceReferences';
import type { IssueSourceChoice } from './sourceApi';

/** The same exact installed choice is used for attachment and mapping review. */
export function IssueSourceProfileSelect({
  choices,
  value,
  disabled = false,
  onChange,
}: {
  choices: readonly IssueSourceChoice[];
  value: string;
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = usePresentation();
  return (
    <SelectMenu
      ariaLabel={t('issueSource.profile')}
      value={value}
      disabled={disabled}
      options={[
        { value: '', label: t('issueSource.chooseProfile') },
        ...choices.map((c) => ({
          value: sourceBindingKey(c.source),
          label: `${c.source.installation.identity} · ${c.native_resource} · ${c.source.configuration.identity} / ${c.source.configuration.version}`,
        })),
      ]}
      onValueChange={onChange}
    />
  );
}
