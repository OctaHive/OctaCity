import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from './useDraftRowKeys';

type Mapping = components['schemas']['WorkSourceStatusMapping'];
type Policy = components['schemas']['WorkSourceReportingPolicy'];

/** Safe effect presentation; mapping values are ordinary configuration, never lifecycle authority. */
export function SourceReporting({
  mapping,
  policy,
  draft,
  onChange,
  single = false,
}: {
  mapping: Mapping | undefined;
  policy: Policy | null | undefined;
  draft: Mapping | null;
  onChange: (mapping: Mapping | null) => void;
  single?: boolean;
}) {
  const { t } = usePresentation();
  const rows = draft?.statuses ?? mapping?.statuses ?? [];
  const keys = useDraftRowKeys(rows);
  function change(index: number, patch: Partial<Mapping['statuses'][number]>) {
    if (draft)
      onChange({
        statuses: draft.statuses.map((row, i) => (i === index ? { ...row, ...patch } : row)),
      });
  }
  return (
    <section aria-label={t('factory.reporterMapping')}>
      <p>{t('factory.reporterReadiness')}</p>
      <p>{t('factory.preserveLabels')}</p>
      {policy && mapping ? (
        <label>
          <input
            type="checkbox"
            checked={draft !== null}
            onChange={(event) => onChange(event.target.checked ? structuredClone(mapping) : null)}
          />
          {t('factory.customizeReporting')}
        </label>
      ) : null}
      {draft && policy ? (
        <>
          {rows.map((row, index) => (
            <fieldset key={keys[index]}>
              <legend>{t('factory.mappingRow', { number: index + 1 })}</legend>
              <label>
                {t('factory.logicalStatus')}
                <input
                  value={row.status}
                  onChange={(event) => change(index, { status: event.target.value })}
                />
              </label>
              {(['add', 'remove'] as const).map((effect) => (
                <fieldset key={effect}>
                  <legend>
                    {t(effect === 'add' ? 'factory.addEffect' : 'factory.removeEffect')}
                  </legend>
                  {policy.labels.map((label) => (
                    <label key={label}>
                      <input
                        type="checkbox"
                        aria-label={t(
                          effect === 'add' ? 'factory.addEffectLabel' : 'factory.removeEffectLabel',
                          { label },
                        )}
                        checked={row[effect].includes(label)}
                        onChange={(event) =>
                          change(index, {
                            [effect]: event.target.checked
                              ? [...row[effect], label]
                              : row[effect].filter((value) => value !== label),
                          })
                        }
                      />
                      {label}
                    </label>
                  ))}
                </fieldset>
              ))}
              <SelectMenu
                ariaLabel={t('factory.resultingState')}
                value={row.state ?? ''}
                options={[
                  { value: '', label: t('factory.preserveState') },
                  ...policy.states.map((state) => ({ value: state, label: state })),
                ]}
                onValueChange={(state) => change(index, { state: state || null })}
              />
              {!single ? (
                <button
                  type="button"
                  onClick={() => onChange({ statuses: rows.filter((_, i) => i !== index) })}
                >
                  {t('factory.removeMapping')}
                </button>
              ) : null}
            </fieldset>
          ))}
          {!single ? (
            <button
              type="button"
              onClick={() =>
                onChange({ statuses: [...rows, { status: '', add: [], remove: [], state: null }] })
              }
            >
              {t('factory.addMapping')}
            </button>
          ) : null}
        </>
      ) : mapping ? (
        <dl>
          {mapping.statuses.map((effect, index) => (
            <div key={keys[index]}>
              <dt>{effect.status}</dt>
              <dd>
                {t('factory.addEffect')}: {effect.add.join(', ') || t('common.none')} ·{' '}
                {t('factory.removeEffect')}: {effect.remove.join(', ') || t('common.none')} ·{' '}
                {t('factory.taskState')}: {effect.state ?? t('factory.preserveState')}
              </dd>
            </div>
          ))}
        </dl>
      ) : (
        <p>{t('factory.reportingUnknown')}</p>
      )}
    </section>
  );
}
