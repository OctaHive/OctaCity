import { replaceEqualDeep } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type S = components['schemas'];
type Reference = S['FactoryImmutableReferenceResource'];

/** Schema identity is part of its digest; reuse a registered contract before normalizing a new one. */
export function equivalentFlowSchema(
  schemas: readonly S['FactoryFlowDataSchema'][],
  shape: S['FactoryFlowValueSchema'],
) {
  return schemas.find((schema) => replaceEqualDeep(schema.shape, shape) === schema.shape);
}

function references(value: unknown, reference: Reference): boolean {
  if (!value || typeof value !== 'object') return false;
  if (
    'identity' in value &&
    value.identity === reference.identity &&
    'version' in value &&
    value.version === reference.version &&
    'digest' in value &&
    value.digest === reference.digest
  )
    return true;
  return Object.values(value).some((child) => references(child, reference));
}

function pinnedDefinitions(
  draft: S['FactoryFlowDefinitionDraft'],
  subflows: readonly S['FactoryFlowDefinitionDetails'][],
): S['FactoryFlowDefinition'][] | null {
  const key = (reference: S['FactoryFlowDefinition']['reference']) =>
    JSON.stringify([reference.id, reference.version, reference.digest]);
  const available = new Map(
    subflows.flatMap(({ flow }) =>
      flow.closure.definitions.map(
        (definition) => [key(definition.reference), definition] as const,
      ),
    ),
  );
  const pending = draft.root.nodes.flatMap((node) => (node.subflow ? [node.subflow] : []));
  const seen = new Set<string>();
  const definitions: S['FactoryFlowDefinition'][] = [];
  while (pending.length) {
    const reference = pending.pop()!;
    const identity = key(reference);
    if (seen.has(identity)) continue;
    seen.add(identity);
    const definition = available.get(identity);
    // A partial explorer page cannot establish that a schema has no live consumers.
    if (!definition) return null;
    definitions.push(definition);
    pending.push(...definition.nodes.flatMap((node) => (node.subflow ? [node.subflow] : [])));
  }
  return definitions;
}

/** Retire replaced inputs only with complete reachability information for their pinned closure. */
export function registerFlowSchemas(
  draft: S['FactoryFlowDefinitionDraft'],
  additions: readonly S['FactoryFlowDataSchema'][],
  replaced: readonly Reference[],
  subflows: readonly S['FactoryFlowDefinitionDetails'][] = [],
): S['FactoryFlowDefinitionDraft'] {
  const { data_schemas: existing, ...consumers } = draft;
  const obsolete = new Set(replaced.map(sourceReferenceKey));
  const children = obsolete.size ? pinnedDefinitions(draft, subflows) : [];
  const seen = new Set<string>();
  const data_schemas = [...existing, ...additions].filter((schema) => {
    const key = sourceReferenceKey(schema.reference);
    if (seen.has(key)) return false;
    seen.add(key);
    return (
      !obsolete.has(key) ||
      children === null ||
      references(consumers, schema.reference) ||
      references(children, schema.reference)
    );
  });
  return { ...draft, data_schemas };
}
