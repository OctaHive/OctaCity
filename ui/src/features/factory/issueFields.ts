import type { components } from '../../../.generated/api/schema';

type S = components['schemas'];

/** Compare canonical field identities without depending on JSON property order or labels. */
export function sameIssueFieldTarget(target: S['FactoryIssueFieldTarget'], value: unknown) {
  if (!value || typeof value !== 'object' || !('kind' in value) || target.kind !== value.kind)
    return false;
  return target.kind === 'property'
    ? Object.keys(value).length === 2 && 'id' in value && target.id === value.id
    : Object.keys(value).length === 1;
}

export function classificationFields(choices: S['FactoryIssueFieldChoices'] | undefined) {
  return (choices?.fields ?? []).flatMap((field) =>
    field.write_target && field.value_key
      ? [{ name: field.value_key, field, target: field.write_target }]
      : [],
  );
}
