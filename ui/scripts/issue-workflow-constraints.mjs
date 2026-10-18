/** Export only authoritative form bounds and choices; business validation remains on the server. */
export function issueWorkflowConstraints(document) {
  const definition = document.components?.schemas?.IssueWorkflowDefinition?.properties;
  const transition = document.components?.schemas?.IssueWorkflowTransition?.properties;
  const integer = (value) => {
    if (!Number.isSafeInteger(value) || value < 1) throw new Error('Missing workflow bound.');
    return value;
  };
  const choices = (value) => {
    if (!Array.isArray(value) || !value.length || !value.every((item) => typeof item === 'string'))
      throw new Error('Missing workflow choices.');
    return value;
  };
  return {
    nameMaximumBytes: integer(definition?.name?.['x-max-utf8-bytes']),
    minimumStatuses: integer(definition?.statuses?.minItems),
    maximumStatuses: integer(definition?.statuses?.maxItems),
    maximumTransitions: integer(definition?.transitions?.maxItems),
    origins: choices(transition?.origins?.items?.enum),
    evidence: choices(transition?.evidence?.items?.enum),
  };
}
