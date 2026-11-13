import { useState } from 'react';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

type Choice = components['schemas']['IssuePropertyChoice'];
/** Stable option IDs survive display edits and retirement; additions allocate an ID once. */
export function IssueChoiceList({
  label,
  choices,
  onChange,
}: {
  label: string;
  choices: Choice[];
  onChange: (choices: Choice[]) => void;
}) {
  const { t } = usePresentation();
  const [name, setName] = useState('');
  const [key, setKey] = useState('');
  const validKey =
    !key ||
    (new RegExp(bounds.catalog.keyPattern).test(key) &&
      new TextEncoder().encode(key).length <= bounds.catalog.keyMaximumBytes &&
      !choices.some((choice) => choice.id === key));
  const validName =
    !!name.trim() &&
    new TextEncoder().encode(name.trim()).length <= bounds.catalog.nameMaximumBytes;
  return (
    <fieldset>
      <legend>{label}</legend>
      {choices.map((choice) => (
        <div key={choice.id}>
          <p>{choice.id}</p>
          <label>
            {t('issueFields.optionName')}
            <input
              value={choice.name}
              onChange={(e) =>
                onChange(
                  choices.map((c) => (c.id === choice.id ? { ...c, name: e.target.value } : c)),
                )
              }
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={choice.retired}
              onChange={(e) =>
                onChange(
                  choices.map((c) =>
                    c.id === choice.id ? { ...c, retired: e.target.checked } : c,
                  ),
                )
              }
            />
            {t('issueFields.retired')}
          </label>
        </div>
      ))}
      <label>
        {t('issueFields.newOption')}
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label>
        {t('issueFields.newOptionId')}
        <input value={key} onChange={(e) => setKey(e.target.value)} />
      </label>
      {!validKey ? (
        <p role="alert">
          {t('issueFields.invalidKey', {
            pattern: bounds.catalog.keyPattern,
            maximum: bounds.catalog.keyMaximumBytes,
          })}
        </p>
      ) : null}
      <button
        type="button"
        disabled={!validKey || !validName || choices.length >= bounds.catalog.maximumChoices}
        onClick={() => {
          onChange([
            ...choices,
            {
              id: key || `option_${crypto.randomUUID().replaceAll('-', '')}`,
              name: name.trim(),
              retired: false,
            },
          ]);
          setName('');
          setKey('');
        }}
      >
        {t('issueFields.addOption')}
      </button>
    </fieldset>
  );
}
