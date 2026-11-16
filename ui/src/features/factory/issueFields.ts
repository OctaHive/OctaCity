import type { components } from '../../../.generated/api/schema';

import { sourceReferenceKey } from '../../shared/sourceReferences';

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

/** Advisory review of current advertised scope; publication remains the authoritative owner. */
export function fieldWriteMatches(
  write: S['FactoryFlowIssueFieldWrite'],
  choices: S['FactoryIssueFieldChoices'] | undefined,
  writer: S['FactoryIssueClassificationChoices'] | undefined,
  projectId: string,
) {
  return (
    !!choices &&
    !!writer &&
    write.form.project_id === projectId &&
    write.form.definition_version === choices.definition_version &&
    write.form.fields.length > 0 &&
    sourceReferenceKey(write.writer) === sourceReferenceKey(writer.plugin) &&
    write.policy === writer.policy &&
    write.form.fields.every(({ field, required }) =>
      classificationFields(choices).some(
        ({ target, field: descriptor }) =>
          sameIssueFieldTarget(target, field) &&
          (required === false || descriptor.required_supported),
      ),
    )
  );
}

/** Stable reader identity; labels and JSON object property order carry no semantics. */
export function issueFieldSourceKey(source: S['FactoryFlowIssueFieldSource']) {
  return JSON.stringify(
    source.kind === 'property'
      ? [source.kind, source.id]
      : source.kind === 'classification'
        ? [source.kind, source.slot]
        : [source.kind],
  );
}
export function directIssueFields(choices: S['FactoryIssueFieldChoices'] | undefined) {
  return choices?.fields.filter((field) => field.source.kind !== 'classification') ?? [];
}
/** Shared advisory resolution of explicit consumer aliases against the authoritative descriptors. */
export function selectedIssueFields(
  fields: Record<string, S['FactoryFlowIssueFieldSelection']>,
  choices: S['FactoryIssueFieldChoices'] | undefined,
) {
  const available = directIssueFields(choices);
  return Object.entries(fields).flatMap(([alias, selection]) => {
    const field = available.find(
      (field) => issueFieldSourceKey(field.source) === issueFieldSourceKey(selection.source),
    );
    return field ? [{ alias, selection, field }] : [];
  });
}
