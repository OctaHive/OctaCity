import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';

export const instructionEntries = (value: components['schemas']['FactoryInstructions']) =>
  typeof value === 'string' ? [value] : value;
/** Ordered text is edited in memory; row identity survives sibling removal. */
export function InstructionFields({
  value,
  onChange,
  label,
  legend = label,
  help,
}: {
  value: components['schemas']['FactoryInstructions'];
  label: string;
  legend?: string;
  help?: string;
  onChange: (value: string[]) => void;
}) {
  const { t } = usePresentation();
  const rows = instructionEntries(value);
  const keys = useDraftRowKeys(rows);
  return (
    <fieldset>
      <legend>{legend}</legend>
      {help ? <p>{help}</p> : null}
      {rows.map((text, index) => (
        <div key={keys[index]}>
          <label>
            {label} {index + 1}
            <textarea
              value={text}
              maxLength={FACTORY_AUTHORING_SCHEMAS.FactoryInstructions['x-max-utf8-bytes']}
              onChange={(event) =>
                onChange(rows.map((row, at) => (at === index ? event.target.value : row)))
              }
            />
          </label>
          <button type="button" onClick={() => onChange(rows.filter((_, at) => at !== index))}>
            {t('factory.removeInstruction', { index: index + 1 })}
          </button>
          <button
            type="button"
            disabled={index === 0}
            onClick={() => {
              const next = [...rows];
              [next[index - 1], next[index]] = [next[index]!, next[index - 1]!];
              onChange(next);
            }}
          >
            {t('factory.moveInstruction', { index: index + 1 })}
          </button>
        </div>
      ))}
      <button
        type="button"
        disabled={rows.length >= FACTORY_AUTHORING_SCHEMAS.FactoryInstructions.oneOf[1].maxItems}
        onClick={() => onChange([...rows, ''])}
      >
        {t('factory.addInstruction', { label: label.toLocaleLowerCase() })}
      </button>
    </fieldset>
  );
}

export function InstructionText({
  value,
}: {
  value: components['schemas']['FactoryInstructions'];
}) {
  const entries = instructionEntries(value);
  const keys = useDraftRowKeys(entries);
  return (
    <ol>
      {entries.map((text, index) => (
        <li key={keys[index]}>{text}</li>
      ))}
    </ol>
  );
}
