import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { issueFieldLabel, type IssueFieldChoice } from './IssueFieldContracts';

type S = components['schemas'];
export type IssueAdmissionPolicy = S['FactoryFlowIssueAdmissionPolicy'];
export type AdmissionField = { alias: string; field: IssueFieldChoice; required: boolean };
const scheduling = FACTORY_AUTHORING_SCHEMAS.FactoryWorkClassification.properties;
const riskValues = scheduling.risk.enum;

function choices(field: AdmissionField) {
  return field.field.shape.type === 'enum'
    ? field.field.shape.values.filter((value): value is string => typeof value === 'string')
    : [];
}
function eligible(field: AdmissionField) {
  return field.required && choices(field).length > 0;
}

/** Validate against the selected published fields; never infer admission from display labels. */
export function validIssueAdmission(
  policy: IssueAdmissionPolicy | undefined,
  fields: readonly AdmissionField[],
) {
  if (!policy) return true;
  const mapped = (field: string, values: Record<string, unknown>) => {
    const selected = fields.find((entry) => entry.alias === field && eligible(entry));
    return (
      selected &&
      Object.keys(values).length > 0 &&
      Object.keys(values).every((value) => choices(selected).includes(value))
    );
  };
  return (
    fields.some(
      ({ alias, field, required }) =>
        alias === policy.priority.field && field.source.kind === 'priority' && required,
    ) &&
    mapped(policy.priority.field, policy.priority.values) &&
    Object.values(policy.priority.values).every(
      (value) =>
        Number.isSafeInteger(value) &&
        value >= scheduling.priority.minimum &&
        value <= scheduling.priority.maximum,
    ) &&
    mapped(policy.risk.field, policy.risk.values) &&
    Object.values(policy.risk.values).every((value) => riskValues.includes(value)) &&
    Object.entries(policy.allowed ?? {}).every(([alias, values]) =>
      mapped(alias, Object.fromEntries(values.map((value) => [value, true]))),
    )
  );
}

/** Shared admission controls for the generic snapshot editor and versioned examples. */
export function IssueAdmission({
  value,
  fields,
  onChange,
}: {
  value: IssueAdmissionPolicy | undefined;
  fields: readonly AdmissionField[];
  onChange: (value: IssueAdmissionPolicy | undefined) => void;
}) {
  const { t } = usePresentation();
  const available = fields.filter(eligible);
  const options = (entries: readonly AdmissionField[]) => [
    { value: '', label: t('factory.chooseInput') },
    ...entries.map(({ alias, field }) => ({
      value: alias,
      label: `${issueFieldLabel(field, t)} · ${alias}`,
    })),
  ];
  return (
    <fieldset>
      <legend>{t('factory.snapshotAdmission')}</legend>
      <p>{t('factory.snapshotAdmissionHelp')}</p>
      <label>
        <input
          type="checkbox"
          checked={!!value}
          onChange={(event) =>
            onChange(
              event.target.checked
                ? {
                    priority: { field: '', values: {} },
                    risk: { field: '', values: {} },
                    allowed: {},
                  }
                : undefined,
            )
          }
        />
        {t('factory.enableSnapshotAdmission')}
      </label>
      {value ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.admissionPriorityField')}
            value={value.priority.field}
            options={options(available.filter(({ field }) => field.source.kind === 'priority'))}
            onValueChange={(field) => onChange({ ...value, priority: { field, values: {} } })}
          />
          {available
            .filter(({ alias }) => alias === value.priority.field)
            .flatMap((field) =>
              choices(field).map((choice) => (
                <label key={choice}>
                  {t('factory.schedulingPriority', { choice })}
                  <input
                    type="number"
                    min={scheduling.priority.minimum}
                    max={scheduling.priority.maximum}
                    value={value.priority.values[choice] ?? ''}
                    onChange={(event) => {
                      const values = { ...value.priority.values };
                      if (event.target.value === '') delete values[choice];
                      else values[choice] = Number(event.target.value);
                      onChange({ ...value, priority: { ...value.priority, values } });
                    }}
                  />
                </label>
              )),
            )}
          <SelectMenu
            ariaLabel={t('factory.admissionRiskField')}
            value={value.risk.field}
            options={options(available)}
            onValueChange={(field) => onChange({ ...value, risk: { field, values: {} } })}
          />
          {available
            .filter(({ alias }) => alias === value.risk.field)
            .flatMap((field) =>
              choices(field).map((choice) => (
                <SelectMenu
                  key={choice}
                  ariaLabel={t('factory.admissionRisk', { choice })}
                  value={value.risk.values[choice] ?? ''}
                  options={[
                    { value: '', label: t('factory.chooseInput') },
                    ...riskValues.map((risk) => ({
                      value: risk,
                      label: t(`triageResult.value.${risk}`),
                    })),
                  ]}
                  onValueChange={(risk) => {
                    const values = { ...value.risk.values };
                    const supported = riskValues.find((entry) => entry === risk);
                    if (supported) values[choice] = supported;
                    else delete values[choice];
                    onChange({ ...value, risk: { ...value.risk, values } });
                  }}
                />
              )),
            )}
          {available.map((field) => (
            <fieldset key={field.alias}>
              <legend>
                {t('factory.admissionAllowed', { field: issueFieldLabel(field.field, t) })}
              </legend>
              {choices(field).map((choice) => (
                <label key={choice}>
                  <input
                    type="checkbox"
                    checked={value.allowed?.[field.alias]?.includes(choice) ?? false}
                    onChange={(event) => {
                      const allowed = { ...value.allowed };
                      const selected = event.target.checked
                        ? [...(allowed[field.alias] ?? []), choice]
                        : (allowed[field.alias] ?? []).filter((entry) => entry !== choice);
                      if (selected.length) allowed[field.alias] = selected;
                      else delete allowed[field.alias];
                      onChange({ ...value, allowed });
                    }}
                  />
                  {t('factory.admissionAllowChoice', { field: field.alias, choice })}
                </label>
              ))}
            </fieldset>
          ))}
          {!validIssueAdmission(value, fields) ? (
            <p role="alert">{t('factory.admissionInvalid')}</p>
          ) : null}
        </>
      ) : null}
    </fieldset>
  );
}
