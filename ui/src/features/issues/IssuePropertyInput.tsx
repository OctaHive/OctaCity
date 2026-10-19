import { SelectMenu } from '../../shared/SelectMenu';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssuePropertyDefinition, IssuePropertyValue } from './api';

/** Scalar controls follow installed Project definitions; no task type has execution semantics. */
export function IssuePropertyInput({
  definition,
  value,
  retained,
  disabled,
  onChange,
}: {
  definition: IssuePropertyDefinition;
  value: IssuePropertyValue | undefined;
  retained: boolean;
  disabled: boolean;
  onChange: (value: IssuePropertyValue | undefined) => void;
}) {
  const { t } = usePresentation();
  const kind = definition.kind;
  let control;
  if (kind.type === 'choice')
    return (
      <div>
        <span>{definition.name}</span>
        <SelectMenu
          ariaLabel={definition.name}
          disabled={disabled}
          value={value?.type === 'choice' ? value.value : ''}
          options={[
            { value: '', label: t('issues.notSet') },
            ...kind.choices
              .filter((c) => !c.retired || (value?.type === 'choice' && value.value === c.id))
              .map((c) => ({ value: c.id, label: c.name })),
          ]}
          onValueChange={(next) => onChange(next ? { type: 'choice', value: next } : undefined)}
        />
      </div>
    );
  else if (kind.type === 'boolean')
    return (
      <div>
        <span>{definition.name}</span>
        <SelectMenu
          ariaLabel={definition.name}
          disabled={disabled}
          value={value?.type === 'boolean' ? String(value.value) : ''}
          options={[
            { value: '', label: t('issues.notSet') },
            { value: 'true', label: t('planning.yes') },
            { value: 'false', label: t('planning.no') },
          ]}
          onValueChange={(next) =>
            onChange(next ? { type: 'boolean', value: next === 'true' } : undefined)
          }
        />
      </div>
    );
  else if (kind.type === 'number')
    control = (
      <input
        disabled={disabled}
        type="number"
        min={retained ? undefined : kind.minimum}
        max={retained ? undefined : kind.maximum}
        step={1}
        value={value?.type === 'number' ? value.value : ''}
        onChange={(e) =>
          onChange(
            e.target.value === '' ? undefined : { type: 'number', value: e.target.valueAsNumber },
          )
        }
      />
    );
  else if (kind.type === 'date')
    control = (
      <input
        disabled={disabled}
        type="date"
        value={value?.type === 'date' ? value.value : ''}
        onChange={(e) =>
          onChange(e.target.value ? { type: 'date', value: e.target.value } : undefined)
        }
      />
    );
  else
    control = (
      <input
        disabled={disabled}
        value={value?.type === 'text' ? value.value : ''}
        onChange={(e) =>
          onChange(e.target.value ? { type: 'text', value: e.target.value } : undefined)
        }
      />
    );
  return (
    <label>
      {definition.name}
      {control}
    </label>
  );
}
