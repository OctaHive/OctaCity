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
  const priority = unwrap(fields?.priority);
  const legacyPriority = priority?.oneOf?.find((item) => item.type === 'integer') ?? priority;
  const bound = boundsFor('Issue');
  const catalog = document.components?.schemas?.IssuePropertyDefinitionSet?.properties;
  const schemas = document.components?.schemas;
  const field = schemas?.IssuePropertyDefinition?.properties;
  const kinds = schemas?.IssuePropertyKind?.oneOf;
  const text = kinds?.find((kind) => kind.properties?.type?.const === 'text')?.properties;
  const number = kinds?.find((kind) => kind.properties?.type?.const === 'number')?.properties;
  const slots = schemas?.IssueClassificationBindings?.properties;
  if (!Array.isArray(kinds) || !slots || !field?.id?.pattern)
    throw new Error('Missing Issue field administration contracts.');
  const sizes = unwrap(document.components?.schemas?.IssuePlanning?.properties?.size)?.enum;
  if (
    !Array.isArray(catalog?.priorities?.default) ||
    !Array.isArray(catalog?.types?.default) ||
    !Array.isArray(sizes)
  )
    throw new Error('Missing Issue built-in choice contracts.');
  const limit = document.paths?.['/api/v1/projects/{project_id}/issues']?.get?.parameters?.find(
    (p) => p.name === 'limit',
  )?.schema;
  return {
    commentMaximumBytes: bound(
      document.components?.schemas?.IssueCommentBody?.['x-max-utf8-bytes'],
    ),
    labelsMaximumCount: bound(
      document.components?.schemas?.IssuePlanning?.properties?.labels?.maxItems,
    ),
    catalog: {
      maximumBytes: bound(schemas.IssuePropertyDefinitionSet['x-max-json-utf8-bytes']),
      maximumFields: bound(catalog.properties?.maxItems),
      maximumChoices: bound(catalog.components?.maxItems),
      keyMaximumBytes: bound(field.id['x-max-utf8-bytes']),
      keyPattern: field.id.pattern,
      nameMaximumBytes: bound(field.name?.['x-max-utf8-bytes']),
      textMaximumBytes: bound(text?.max_bytes?.maximum),
      numberMinimum: -bound(-number?.minimum?.minimum),
      numberMaximum: bound(number?.maximum?.maximum),
      kinds: kinds.map((kind) => kind.properties.type.const),
      classificationSlots: Object.keys(slots),
    },
    defaultPriorities: catalog.priorities.default,
    defaultTypes: catalog.types.default,
    sizes,
    titleMaximumBytes: bound(fields?.title?.['x-max-utf8-bytes']),
    descriptionMaximumBytes: bound(fields?.description?.['x-max-utf8-bytes']),
    acceptanceMaximumBytes: bound(unwrap(fields?.acceptance)?.['x-max-utf8-bytes']),
    priorityMinimum: bound(legacyPriority?.minimum, 0),
    priorityMaximum: bound(legacyPriority?.maximum),
    defaultPageSize: bound(limit?.default),
    outcomeConclusionMaximumBytes: bound(
      document.components?.schemas?.IssueOutcomeCard?.properties?.conclusion?.['x-max-utf8-bytes'],
    ),
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
