import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { issueFieldLabel, IssueFieldType } from './IssueFieldContracts';
import { directIssueFields, issueFieldSourceKey, selectedIssueFields } from './issueFields';

type S = components['schemas'];
/** Consumer aliases select ordinary Project inputs, independently of any producer or writer. */
export function SnapshotFieldSelectors({
  fields,
  choices,
  onChange,
}: {
  fields: Record<string, S['FactoryFlowIssueFieldSelection']>;
  choices: S['FactoryIssueFieldChoices'];
  onChange: (fields: Record<string, S['FactoryFlowIssueFieldSelection']>) => void;
}) {
  const { t } = usePresentation();
  const offered = directIssueFields(choices);
  const selected = selectedIssueFields(fields, choices);
  return (
    <>
      {Object.entries(fields).map(([alias, selection]) => {
        const entry = selected.find((field) => field.alias === alias);
        return (
          <fieldset key={alias}>
            <legend>{alias}</legend>
            <SelectMenu
              ariaLabel={t('factory.snapshotFieldSource', { alias })}
              value={entry ? issueFieldSourceKey(selection.source) : ''}
              options={[
                { value: '', label: t('factory.chooseInput') },
                ...offered.map((field) => ({
                  value: issueFieldSourceKey(field.source),
                  label: issueFieldLabel(field, t),
                })),
              ]}
              onValueChange={(key) => {
                const field = offered.find((field) => issueFieldSourceKey(field.source) === key);
                if (field && key !== issueFieldSourceKey(selection.source)) {
                  onChange({ ...fields, [alias]: { ...selection, source: field.source } });
                }
              }}
            />
            {entry && ['enum', 'string', 'integer', 'boolean'].includes(entry.field.shape.type) ? (
              <IssueFieldType field={entry.field} />
            ) : null}
            <label>
              <input
                type="checkbox"
                checked={selection.required}
                disabled={!entry || (!entry.field.required_supported && !selection.required)}
                onChange={(event) =>
                  onChange({ ...fields, [alias]: { ...selection, required: event.target.checked } })
                }
              />
              {t('factory.requiredIssueField', { name: alias })}
            </label>
          </fieldset>
        );
      })}
    </>
  );
}
