import { useState } from 'react';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import type { IssuePropertyDefinition } from './api';
import { IssueChoiceList } from './IssueChoiceList';

export const FIELD_KIND_LABELS = {
  text: 'issueFields.text',
  choice: 'issueFields.choice',
  boolean: 'issueFields.boolean',
  number: 'issueFields.number',
  date: 'issueFields.date',
} as const;
type Kind = IssuePropertyDefinition['kind'];
/** Published field IDs/types have no rename/type-conversion controls; retire identities instead. */
export function IssueFieldDefinitions({
  fields,
  onChange,
}: {
  fields: IssuePropertyDefinition[];
  onChange: (fields: IssuePropertyDefinition[]) => void;
}) {
  const { t } = usePresentation();
  const [name, setName] = useState('');
  const [type, setType] = useState<Kind['type']>('text');
  const replace = (field: IssuePropertyDefinition) =>
    onChange(fields.map((old) => (old.id === field.id ? field : old)));
  return (
    <fieldset>
      <legend>{t('issueFields.custom')}</legend>
      {fields.map((field) => (
        <fieldset key={field.id}>
          <legend>{field.name}</legend>
          <p>
            {field.id} · {t(FIELD_KIND_LABELS[field.kind.type])}
          </p>
          <label>
            {t('issueFields.fieldName')}
            <input
              value={field.name}
              onChange={(e) => replace({ ...field, name: e.target.value })}
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={field.retired}
              onChange={(e) => replace({ ...field, retired: e.target.checked })}
            />
            {t('issueFields.retireField')}
          </label>
          {field.kind.type === 'text' ? (
            <label>
              {t('issueFields.textLimit')}
              <input
                type="number"
                min={1}
                max={bounds.catalog.textMaximumBytes}
                value={Number.isNaN(field.kind.max_bytes) ? '' : field.kind.max_bytes}
                onChange={(e) =>
                  replace({ ...field, kind: { type: 'text', max_bytes: e.target.valueAsNumber } })
                }
              />
            </label>
          ) : null}
          {field.kind.type === 'number' ? (
            <>
              <label>
                {t('issueFields.minimum')}
                <input
                  type="number"
                  min={bounds.catalog.numberMinimum}
                  max={bounds.catalog.numberMaximum}
                  value={Number.isNaN(field.kind.minimum) ? '' : field.kind.minimum}
                  onChange={(e) => {
                    if (field.kind.type === 'number')
                      replace({
                        ...field,
                        kind: { ...field.kind, minimum: e.target.valueAsNumber },
                      });
                  }}
                />
              </label>
              <label>
                {t('issueFields.maximum')}
                <input
                  type="number"
                  min={bounds.catalog.numberMinimum}
                  max={bounds.catalog.numberMaximum}
                  value={Number.isNaN(field.kind.maximum) ? '' : field.kind.maximum}
                  onChange={(e) => {
                    if (field.kind.type === 'number')
                      replace({
                        ...field,
                        kind: { ...field.kind, maximum: e.target.valueAsNumber },
                      });
                  }}
                />
              </label>
            </>
          ) : null}
          {field.kind.type === 'choice' ? (
            <IssueChoiceList
              label={t('issueFields.options')}
              choices={field.kind.choices}
              onChange={(choices) => replace({ ...field, kind: { type: 'choice', choices } })}
            />
          ) : null}
        </fieldset>
      ))}
      <label>
        {t('issueFields.newFieldName')}
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <SelectMenu
        ariaLabel={t('issueFields.fieldType')}
        value={type}
        onValueChange={setType}
        options={bounds.catalog.kinds.map((value) => ({
          value,
          label: t(FIELD_KIND_LABELS[value]),
        }))}
      />
      <button
        type="button"
        disabled={
          !name.trim() ||
          new TextEncoder().encode(name.trim()).length > bounds.catalog.nameMaximumBytes ||
          fields.length >= bounds.catalog.maximumFields
        }
        onClick={() => {
          const kind: Kind =
            type === 'text'
              ? { type, max_bytes: bounds.catalog.textMaximumBytes }
              : type === 'number'
                ? { type, minimum: 0, maximum: bounds.catalog.numberMaximum }
                : type === 'choice'
                  ? { type, choices: [] }
                  : { type };
          onChange([
            ...fields,
            {
              id: `field_${crypto.randomUUID().replaceAll('-', '')}`,
              name: name.trim(),
              retired: false,
              kind,
            },
          ]);
          setName('');
        }}
      >
        {t('issueFields.addField')}
      </button>
    </fieldset>
  );
}
