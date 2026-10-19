/** Derive task form bounds from the published OpenAPI contract, preserving their units. */
export function issueConstraints(document) {
  const fields = document.components?.schemas?.IssueContent?.properties;
  const unwrap = (schema) => schema?.anyOf?.find((item) => item.type !== 'null') ?? schema;
  const bound = (value, minimum = 1) => {
    if (!Number.isSafeInteger(value) || value < minimum)
      throw new Error('Missing Issue form bound.');
    return value;
  };
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
