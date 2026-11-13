import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Definition = components['schemas']['IssuePropertyDefinitionSet'];
/** Semantics are optional, distinct choice-field references; built-ins remain separate. */
export function IssueFieldSemantics({
  definition,
  onChange,
}: {
  definition: Definition;
  onChange: (bindings: NonNullable<Definition['classification']>) => void;
}) {
  const { t } = usePresentation();
  const bindings = definition.classification ?? {};
  return (
    <fieldset>
      <legend>{t('issueFields.semantics')}</legend>
      <p>{t('issueFields.semanticHelp')}</p>
      {bounds.catalog.classificationSlots.map((slot) => {
        const selected = bindings[slot] ?? '';
        const fields = definition.properties.filter(
          (field) =>
            field.kind.type === 'choice' &&
            (!field.retired || field.id === selected) &&
            !Object.entries(bindings).some(([other, id]) => other !== slot && id === field.id),
        );
        return (
          <SelectMenu
            key={slot}
            ariaLabel={t(`issueFields.binding.${slot}`)}
            value={selected}
            options={[
              { value: '', label: t('issues.notSet') },
              ...fields.map((field) => ({ value: field.id, label: field.name })),
              ...(selected && !fields.some((field) => field.id === selected)
                ? [{ value: selected, label: t('planning.unavailableValue') }]
                : []),
            ]}
            onValueChange={(value) => {
              const next = { ...bindings };
              if (value) next[slot] = value;
              else delete next[slot];
              onChange(next);
            }}
          />
        );
      })}
    </fieldset>
  );
}
