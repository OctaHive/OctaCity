const unwrap = (schema) => schema?.anyOf?.find((item) => item.type !== 'null') ?? schema;
function boundsFor(subject) {
  return (value, minimum = 1) => {
    if (!Number.isSafeInteger(value) || value < minimum)
      throw new Error(`Missing ${subject} form bound.`);
    return value;
  };
}
/** Derive task form bounds from the published OpenAPI contract, preserving their units. */
export function issueConstraints(document) {
  const fields = document.components?.schemas?.IssueContent?.properties;
  const bound = boundsFor('Issue');
  const limit = document.paths?.['/api/v1/projects/{project_id}/issues']?.get?.parameters?.find(
    (p) => p.name === 'limit',
  )?.schema;
  return {
    labelsMaximumCount: bound(
      document.components?.schemas?.IssuePlanning?.properties?.labels?.maxItems,
    ),
    titleMaximumBytes: bound(fields?.title?.['x-max-utf8-bytes']),
    descriptionMaximumBytes: bound(fields?.description?.['x-max-utf8-bytes']),
    acceptanceMaximumBytes: bound(unwrap(fields?.acceptance)?.['x-max-utf8-bytes']),
    priorityMinimum: bound(unwrap(fields?.priority)?.minimum, 0),
    priorityMaximum: bound(unwrap(fields?.priority)?.maximum),
    defaultPageSize: bound(limit?.default),
    reasonMaximumBytes: bound(
      unwrap(document.components?.schemas?.TransitionIssueRequest?.properties?.reason)?.[
        'x-max-utf8-bytes'
      ],
    ),
  };
}

/** Source forms preserve common predicate/effect limits and the separate Issue mapping ceiling. */
export function issueSourceConstraints(document) {
  const schemas = document.components?.schemas;
  const predicate = schemas?.WorkSourcePredicate?.properties;
  const bound = boundsFor('Issue Source');
  return {
    mappingMaximumBytes: bound(schemas?.IssueSourceMappingDefinition?.['x-max-utf8-bytes']),
    pullMaximumRows: bound(schemas?.IssueSourceStatusMapping?.properties?.pull?.maxItems),
    pushMaximumRows: bound(schemas?.WorkSourceStatusMapping?.properties?.statuses?.maxItems),
    predicateMaximumLabels: bound(predicate?.all_of?.maxItems),
    effectMaximumLabels: bound(schemas?.WorkSourceStatusEffects?.properties?.add?.maxItems),
    stateMaximumBytes: bound(unwrap(predicate?.state)?.['x-max-utf8-bytes']),
    labelMaximumBytes: bound(predicate?.all_of?.items?.['x-max-utf8-bytes']),
    nativeIdentityMaximumBytes: bound(
      schemas?.IssueSourceItem?.properties?.item?.['x-max-utf8-bytes'],
    ),
  };
}
