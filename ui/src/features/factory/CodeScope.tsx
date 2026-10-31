import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
type Scope = components['schemas']['FactoryFlowCodeScope'];
const bounds = FACTORY_AUTHORING_SCHEMAS;
/** Path/range requests are validated by the shared owner; selecting a ceiling never verifies execution support. */
export function CodeScope({ value, onChange }: { value: Scope; onChange: (scope: Scope) => void }) {
  const { t } = usePresentation();
  const entries = value.kind === 'selected' ? value.entries : [];
  const keys = useDraftRowKeys(entries);
  return (
    <fieldset>
      <legend>{t('factory.codeScope')}</legend>
      <SelectMenu
        ariaLabel={t('factory.codeScopeMode')}
        value={value.kind}
        options={[
          { value: 'repository', label: t('factory.codeWholeRepository') },
          { value: 'selected', label: t('factory.codeSelectedPaths') },
        ]}
        onValueChange={(kind) =>
          onChange(
            kind === 'repository'
              ? { kind: 'repository' }
              : { kind: 'selected', entries: [{ kind: 'path', path: '' }] },
          )
        }
      />
      {entries.map((entry, index) => (
        <fieldset key={keys[index]}>
          <label>
            {t('factory.codePath')}
            <input
              maxLength={bounds.FactoryFlowCodeSelection.oneOf[0].properties.path.maxLength}
              value={entry.path}
              onChange={(e) =>
                onChange({
                  kind: 'selected',
                  entries: entries.map((item, i) =>
                    i === index ? { ...item, path: e.target.value } : item,
                  ),
                })
              }
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={entry.kind === 'range'}
              onChange={(e) =>
                onChange({
                  kind: 'selected',
                  entries: entries.map((item, i) =>
                    i !== index
                      ? item
                      : e.target.checked
                        ? {
                            kind: 'range',
                            path: entry.path,
                            lines: { first_line: 1, last_line: 1 },
                          }
                        : { kind: 'path', path: entry.path },
                  ),
                })
              }
            />
            {t('factory.codeLineRange')}
          </label>
          {entry.kind === 'range'
            ? (['first_line', 'last_line'] as const).map((key) => (
                <label key={key}>
                  {t(key === 'first_line' ? 'factory.codeFirstLine' : 'factory.codeLastLine')}
                  <input
                    type="number"
                    min={bounds.FactoryRepositoryLineRange.properties[key].minimum}
                    max={bounds.FactoryRepositoryLineRange.properties[key].maximum}
                    value={entry.lines[key]}
                    onChange={(e) =>
                      onChange({
                        kind: 'selected',
                        entries: entries.map((item, i) =>
                          i === index
                            ? { ...entry, lines: { ...entry.lines, [key]: Number(e.target.value) } }
                            : item,
                        ),
                      })
                    }
                  />
                </label>
              ))
            : null}
          <button
            type="button"
            onClick={() =>
              onChange({ kind: 'selected', entries: entries.filter((_, i) => i !== index) })
            }
          >
            {t('factory.codeRemovePath')}
          </button>
        </fieldset>
      ))}
      {value.kind === 'selected' ? (
        <button
          type="button"
          disabled={
            entries.length >= bounds.FactoryFlowCodeScope.oneOf[1].properties.entries.maxItems
          }
          onClick={() =>
            onChange({ kind: 'selected', entries: [...entries, { kind: 'path', path: '' }] })
          }
        >
          {t('factory.codeAddPath')}
        </button>
      ) : null}
    </fieldset>
  );
}
