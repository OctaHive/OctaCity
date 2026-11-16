import { sourceReferenceKey } from '../../shared/sourceReferences';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { issueFieldLabel, IssueFieldType } from './IssueFieldContracts';
import { classificationFields, sameIssueFieldTarget } from './issueFields';

type S = components['schemas'];
export type FieldWrite = S['FactoryFlowIssueFieldWrite'];

/** Only supported server-advertised identities are selectable; the server derives their form. */
export function FillIssueFields({
  value,
  choices,
  writer,
  projectId,
  onChange,
}: {
  value: FieldWrite | null;
  choices: S['FactoryIssueFieldChoices'] | undefined;
  writer: S['FactoryIssueClassificationChoices'] | undefined;
  projectId: string;
  onChange: (write: FieldWrite) => void;
}) {
  const { t } = usePresentation();
  if (!choices || !writer) return <p role="alert">{t('factory.issueFieldsUnavailable')}</p>;
  const targets = classificationFields(choices);
  const form = value?.form ?? {
    project_id: projectId,
    definition_version: choices.definition_version,
    fields: [],
  };
  const update = (fields: typeof form.fields) =>
    onChange({ writer: writer.plugin, policy: writer.policy, form: { ...form, fields } });
  const stale =
    form.definition_version !== choices.definition_version ||
    (!!value &&
      (sourceReferenceKey(value.writer) !== sourceReferenceKey(writer.plugin) ||
        value.policy !== writer.policy));
  const missing = form.fields.filter(
    ({ field }) => !targets.some(({ target }) => sameIssueFieldTarget(target, field)),
  );
  return (
    <fieldset>
      <legend>{t('factory.fillIssueFields')}</legend>
      <p>{t('factory.fillIssueFieldsHelp')}</p>
      <p>{t('planning.definition', { version: form.definition_version })}</p>
      {stale ? (
        <>
          <p role="alert">{t('factory.fieldFormStale')}</p>
          <button
            type="button"
            onClick={() =>
              onChange({
                writer: writer.plugin,
                policy: writer.policy,
                form: { ...form, definition_version: choices.definition_version },
              })
            }
          >
            {t('factory.reconcileFieldForm')}
          </button>
        </>
      ) : null}
      {targets.map(({ field, target, name }) => {
        const selected = form.fields.find((item) => sameIssueFieldTarget(target, item.field));
        const label = issueFieldLabel(field, t);
        return (
          <fieldset key={name}>
            <label>
              <input
                type="checkbox"
                checked={!!selected}
                disabled={stale}
                onChange={(event) =>
                  update(
                    event.target.checked
                      ? [...form.fields, { field: target, required: true }]
                      : form.fields.filter((item) => !sameIssueFieldTarget(target, item.field)),
                  )
                }
              />
              {label}
            </label>
            <IssueFieldType field={field} />
            {selected ? (
              <label>
                <input
                  type="checkbox"
                  checked={selected.required ?? true}
                  disabled={stale || (!field.required_supported && selected.required === false)}
                  onChange={(event) =>
                    update(
                      form.fields.map((item) =>
                        sameIssueFieldTarget(target, item.field)
                          ? { ...item, required: event.target.checked }
                          : item,
                      ),
                    )
                  }
                />
                {t('factory.requiredIssueField', { name: label })}
              </label>
            ) : null}
          </fieldset>
        );
      })}
      {missing.map(({ field }) => (
        <p key={JSON.stringify(field)} role="alert">
          {t('planning.unavailableValue')} · {field.kind === 'property' ? field.id : field.kind}{' '}
          <button
            type="button"
            onClick={() =>
              update(form.fields.filter((item) => !sameIssueFieldTarget(field, item.field)))
            }
          >
            {t('factory.removeField')}
          </button>
        </p>
      ))}
      <details>
        <summary>{t('factory.exactFieldForm')}</summary>
        <pre>{JSON.stringify(form, null, 2)}</pre>
      </details>
    </fieldset>
  );
}

/** Publication presents the same form that generated the accepted result and save scope. */
export function IssueFieldSaveReview({
  write,
  choices,
}: {
  write: FieldWrite;
  choices: S['FactoryIssueFieldChoices'];
}) {
  const { t } = usePresentation();
  return (
    <section>
      <p>{t('factory.fillIssueFieldsHelp')}</p>
      <p>{t('planning.definition', { version: write.form.definition_version })}</p>
      <ul>
        {write.form.fields.map(({ field, required }) => {
          const descriptor = classificationFields(choices).find(({ target }) =>
            sameIssueFieldTarget(target, field),
          )?.field;
          return (
            <li key={JSON.stringify(field)}>
              <span>{descriptor ? issueFieldLabel(descriptor, t) : field.kind}</span> ·{' '}
              {t(required !== false ? 'factory.required' : 'issueResults.optional')}
              {descriptor ? <IssueFieldType field={descriptor} /> : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
