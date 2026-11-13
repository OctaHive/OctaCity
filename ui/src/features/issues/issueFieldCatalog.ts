import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';

type Definition = components['schemas']['IssuePropertyDefinitionSet'];
const byteLength = (value: string) => new TextEncoder().encode(value).length;
const validName = (value: string) =>
  !!value &&
  value === value.trim() &&
  !/\p{Cc}/u.test(value) &&
  byteLength(value) <= bounds.catalog.nameMaximumBytes;
/** Advisory form validation uses published bounds; the server retains publication authority. */
export function fieldCatalogProblems(definition: Definition): string[] {
  const problems: string[] = [];
  const choicesValid = (choices: components['schemas']['IssuePropertyChoice'][]) =>
    choices.length <= bounds.catalog.maximumChoices && choices.every((c) => validName(c.name));
  for (const list of ['priorities', 'components', 'types', 'labels'] as const)
    if (!choicesValid(definition[list] ?? [])) problems.push(list);
  if (definition.properties.length > bounds.catalog.maximumFields) problems.push('properties');
  for (const field of definition.properties) {
    const kind = field.kind;
    if (
      !validName(field.name) ||
      (kind.type === 'choice' && (!kind.choices.length || !choicesValid(kind.choices))) ||
      (kind.type === 'text' &&
        (!Number.isSafeInteger(kind.max_bytes) ||
          kind.max_bytes < 1 ||
          kind.max_bytes > bounds.catalog.textMaximumBytes)) ||
      (kind.type === 'number' &&
        (!Number.isSafeInteger(kind.minimum) ||
          !Number.isSafeInteger(kind.maximum) ||
          kind.minimum > kind.maximum ||
          kind.minimum < bounds.catalog.numberMinimum ||
          kind.maximum > bounds.catalog.numberMaximum))
    )
      problems.push(field.name || field.id);
  }
  const bound = Object.values(definition.classification ?? {}).filter((id): id is string => !!id);
  if (
    new Set(bound).size !== bound.length ||
    bound.some((id) => !definition.properties.some((p) => p.id === id && p.kind.type === 'choice'))
  )
    problems.push('classification');
  if (byteLength(JSON.stringify(definition)) > bounds.catalog.maximumBytes)
    problems.push('catalog');
  return problems;
}

type Row =
  components['schemas']['IssuePropertyChoice'] | components['schemas']['IssuePropertyDefinition'];
function rowSignature(row: Row | undefined) {
  if (!row) return '';
  const base: unknown[] = [row.id, row.name, row.retired];
  if ('kind' in row) {
    const kind = row.kind;
    base.push(
      kind.type,
      kind.type === 'text'
        ? kind.max_bytes
        : kind.type === 'number'
          ? [kind.minimum, kind.maximum]
          : kind.type === 'choice'
            ? kind.choices.map((c) => [c.id, c.name, c.retired])
            : null,
    );
  }
  return JSON.stringify(base);
}
/** Three-way reconciliation retains remote rows unless the operator changed that stable identity. */
export function reconcileFieldCatalog(
  base: Definition,
  draft: Definition,
  current: Definition,
  decisions: Readonly<Record<string, 'local' | 'current'>> = {},
) {
  const conflicts: string[] = [];
  function rows<T extends Row>(namespace: string, previous: T[], local: T[], remote: T[]) {
    const result = remote.map((item) => {
      const original = previous.find((p) => p.id === item.id);
      const edited = local.find((p) => p.id === item.id);
      if (!edited || rowSignature(edited) === rowSignature(original)) return item;
      if (
        rowSignature(item) !== rowSignature(original) &&
        rowSignature(item) !== rowSignature(edited)
      ) {
        const key = `${namespace}/${item.id}`;
        if (decisions[key] === 'current') return item;
        if (decisions[key] !== 'local') conflicts.push(key);
      }
      const retained = { ...edited };
      // Existing immutable choice IDs cannot be removed by retaining a field-level edit.
      if (
        'kind' in retained &&
        retained.kind.type === 'choice' &&
        'kind' in item &&
        item.kind.type === 'choice'
      ) {
        const selected = retained.kind.choices;
        retained.kind = {
          type: 'choice',
          choices: [
            ...selected,
            ...item.kind.choices.filter((choice) => !selected.some((c) => c.id === choice.id)),
          ],
        };
      }
      return retained;
    });
    return [...result, ...local.filter((item) => !remote.some((r) => r.id === item.id))];
  }
  const definition: Definition = {
    ...current,
    priorities: rows(
      'priorities',
      base.priorities ?? [],
      draft.priorities ?? [],
      current.priorities ?? [],
    ),
    components: rows(
      'components',
      base.components ?? [],
      draft.components ?? [],
      current.components ?? [],
    ),
    types: rows('types', base.types ?? [], draft.types ?? [], current.types ?? []),
    properties: rows('properties', base.properties, draft.properties, current.properties),
  };
  const bindings = { ...current.classification };
  for (const slot of bounds.catalog.classificationSlots) {
    const original = base.classification?.[slot] ?? null;
    const edited = draft.classification?.[slot] ?? null;
    const remote = current.classification?.[slot] ?? null;
    if (edited === original) continue;
    if (remote !== original && remote !== edited) {
      const key = `classification/${slot}`;
      if (decisions[key] === 'current') continue;
      if (decisions[key] !== 'local') conflicts.push(key);
    }
    if (edited) bindings[slot] = edited;
    else delete bindings[slot];
  }
  if (Object.keys(bindings).length) definition.classification = bindings;
  else delete definition.classification;
  return { definition, conflicts };
}
