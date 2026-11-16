import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';

type S = components['schemas'];
export type SnapshotEvidenceSelections = Record<string, S['FactoryFlowIssueEvidenceSelection']>;
const bounds = FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueEvidenceSelection.properties;
const key = FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueEvidenceSelection.properties.field;
const digest = FACTORY_AUTHORING_SCHEMAS.FactoryDigest;
const validReference = (ref: S['FactoryImmutableReferenceResource']) =>
  [ref.identity, ref.version].every((value) => value.trim().length > 0) &&
  new RegExp(digest.pattern).test(ref.digest);
/** Advisory checks for the existing bounded policy, never verification of a command or report. */
export function validSnapshotEvidence(
  value: SnapshotEvidenceSelections,
  fields: readonly { alias: string }[],
) {
  return Object.values(value).every(
    (selection) =>
      fields.some((field) => field.alias === selection.field) &&
      Number.isInteger(selection.maximum) &&
      selection.maximum >= bounds.maximum.minimum &&
      selection.maximum <= bounds.maximum.maximum &&
      (!selection.observation ||
        (validReference(selection.observation.environment) &&
          validReference(selection.observation.criterion))),
  );
}
/** Explicit evidence policy uses the common trusted observation reader; field values confer no proof. */
export function SnapshotEvidence({
  value,
  fields,
  onChange,
}: {
  value: SnapshotEvidenceSelections;
  fields: readonly { alias: string; label: string }[];
  onChange: (value: SnapshotEvidenceSelections, rename?: { from: string; to: string }) => void;
}) {
  const { t } = usePresentation();
  const rows = useDraftRowKeys(Object.values(value));
  const update = (alias: string, selection: S['FactoryFlowIssueEvidenceSelection']) =>
    onChange({ ...value, [alias]: selection });
  const empty = () => ({ identity: '', version: '', digest: '' });
  return (
    <fieldset>
      <legend>{t('factory.snapshotEvidence')}</legend>
      <p>{t('factory.snapshotEvidenceHelp')}</p>
      {Object.entries(value).map(([alias, selection], index) => (
        <fieldset key={rows[index]}>
          <label>
            {t('factory.evidenceAlias')}
            <input
              value={alias}
              maxLength={key.maxLength}
              onChange={(event) => {
                const name = event.target.value;
                if (name !== alias && !Object.hasOwn(value, name))
                  onChange(
                    Object.fromEntries(
                      Object.entries(value).map(([old, row]) => [old === alias ? name : old, row]),
                    ),
                    { from: alias, to: name },
                  );
              }}
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.evidenceField')}
            value={fields.some((f) => f.alias === selection.field) ? selection.field : ''}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...fields.map((f) => ({ value: f.alias, label: f.label })),
            ]}
            onValueChange={(field) => update(alias, { ...selection, field })}
          />
          <label>
            {t('factory.evidenceMaximum')}
            <input
              type="number"
              min={bounds.maximum.minimum}
              max={bounds.maximum.maximum}
              value={selection.maximum || ''}
              onChange={(event) =>
                update(alias, { ...selection, maximum: Number(event.target.value) })
              }
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={selection.required}
              onChange={(event) => update(alias, { ...selection, required: event.target.checked })}
            />
            {t('factory.requiredIssueField', { name: alias })}
          </label>
          <label>
            <input
              type="checkbox"
              checked={!!selection.observation}
              onChange={(event) => {
                const { observation: _removed, ...rest } = selection;
                void _removed;
                update(
                  alias,
                  event.target.checked
                    ? {
                        ...selection,
                        observation: {
                          criterion: empty(),
                          environment: empty(),
                          finding: 'matched',
                        },
                      }
                    : rest,
                );
              }}
            />
            {t('factory.requireObservation')}
          </label>
          {selection.observation ? (
            <fieldset>
              <legend>{t('factory.trustedObservation')}</legend>
              {(['environment', 'criterion'] as const).map((kind) => (
                <fieldset key={kind}>
                  <legend>
                    {t(
                      kind === 'environment'
                        ? 'factory.evidenceEnvironment'
                        : 'factory.evidenceCriterion',
                    )}
                  </legend>
                  {(['identity', 'version', 'digest'] as const).map((part) => (
                    <label key={part}>
                      {t(
                        part === 'identity'
                          ? 'factory.referenceIdentity'
                          : part === 'version'
                            ? 'factory.referenceVersion'
                            : 'factory.referenceDigest',
                      )}
                      <input
                        value={selection.observation![kind][part]}
                        maxLength={part === 'digest' ? digest.maxLength : undefined}
                        onChange={(event) =>
                          update(alias, {
                            ...selection,
                            observation: {
                              ...selection.observation!,
                              [kind]: {
                                ...selection.observation![kind],
                                [part]: event.target.value,
                              },
                            },
                          })
                        }
                      />
                    </label>
                  ))}
                </fieldset>
              ))}
              <SelectMenu
                ariaLabel={t('factory.evidenceFinding')}
                value={selection.observation.finding}
                options={(['matched', 'not_matched', 'indeterminate'] as const).map((finding) => ({
                  value: finding,
                  label: t(
                    finding === 'matched'
                      ? 'factory.findingMatched'
                      : finding === 'not_matched'
                        ? 'factory.findingNotMatched'
                        : 'factory.findingIndeterminate',
                  ),
                }))}
                onValueChange={(finding) => {
                  if (
                    finding === 'matched' ||
                    finding === 'not_matched' ||
                    finding === 'indeterminate'
                  )
                    update(alias, {
                      ...selection,
                      observation: { ...selection.observation!, finding },
                    });
                }}
              />
            </fieldset>
          ) : null}
          <button
            type="button"
            onClick={() =>
              onChange(Object.fromEntries(Object.entries(value).filter(([name]) => name !== alias)))
            }
          >
            {t('factory.removeEvidence')}
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={
          Object.keys(value).length >=
          FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueSnapshotInput.properties.evidence.maxProperties
        }
        onClick={() => {
          const alias = Array.from(
            { length: Object.keys(value).length + 1 },
            (_, index) => `evidence_${index + 1}`,
          ).find((name) => !Object.hasOwn(value, name))!;
          onChange({
            ...value,
            [alias]: { field: fields[0]?.alias ?? '', maximum: 1, required: true },
          });
        }}
      >
        {t('factory.addEvidence')}
      </button>
    </fieldset>
  );
}
