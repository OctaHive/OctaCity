import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

/** Closed scalar controls shared by constants and finite schema alternatives. */
export function ScalarEditor({
  value,
  onChange,
  typeLabel,
  valueLabel,
}: {
  value: unknown;
  onChange: (value: string | number | boolean | null) => void;
  typeLabel: string;
  valueLabel: string;
}) {
  const { t } = usePresentation();
  return (
    <>
      <SelectMenu
        ariaLabel={typeLabel}
        value={value === null ? 'null' : typeof value}
        options={[
          { value: 'boolean', label: t('factory.boolean') },
          { value: 'number', label: t('factory.integer') },
          { value: 'string', label: t('factory.string') },
          { value: 'null', label: t('factory.null') },
        ]}
        onValueChange={(kind) =>
          onChange(
            kind === 'boolean' ? false : kind === 'number' ? 0 : kind === 'string' ? '' : null,
          )
        }
      />
      {typeof value === 'boolean' ? (
        <label>
          <input
            type="checkbox"
            checked={value}
            onChange={(event) => onChange(event.target.checked)}
          />
          {valueLabel}
        </label>
      ) : typeof value === 'string' ? (
        <label>
          {valueLabel}
          <input
            maxLength={8192}
            value={value}
            onChange={(event) => onChange(event.target.value)}
          />
        </label>
      ) : typeof value === 'number' ? (
        <label>
          {valueLabel}
          <input
            type="number"
            min={-Number.MAX_SAFE_INTEGER}
            max={Number.MAX_SAFE_INTEGER}
            value={value}
            onChange={(event) => onChange(Number(event.target.value))}
          />
        </label>
      ) : value === null ? null : (
        <p>{t('factory.structuredConstant')}</p>
      )}
    </>
  );
}
