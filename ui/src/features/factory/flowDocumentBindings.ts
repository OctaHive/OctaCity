import type { components } from '../../../.generated/api/schema';

type Artifact = components['schemas']['FactoryArtifactReference'];
type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Selection = components['schemas']['FactoryFlowContextSelection'];

/** Recognizes only literal document references, including historical object-shaped mappings. */
export function documentSelections(node: Node, processManaged = false) {
  return Object.entries(node.context_binding?.entries ?? {}).flatMap(([key, selection]) => {
    if (processManaged && (key === 'process.goals' || key === 'process.criteria')) return [];
    if (selection.source_kind !== 'policy' && selection.source_kind !== 'specification') return [];
    const mapping = selection.source.value;
    let value: unknown;
    if (mapping.mapping === 'constant') {
      const literal = mapping.value;
      if (
        literal &&
        typeof literal === 'object' &&
        'kind' in literal &&
        literal.kind === 'artifact' &&
        'value' in literal
      )
        value = literal.value;
    } else if (mapping.mapping === 'object') {
      const kind = mapping.fields.kind;
      const reference = mapping.fields.value;
      if (
        kind?.mapping === 'constant' &&
        kind.value === 'artifact' &&
        reference?.mapping === 'constant'
      )
        value = reference.value;
    }
    return isArtifact(value) ? [{ key, selection, artifact: value }] : [];
  });
}

function isArtifact(value: unknown): value is Artifact {
  return (
    value !== null &&
    typeof value === 'object' &&
    'artifact_id' in value &&
    typeof value.artifact_id === 'string' &&
    'content_digest' in value &&
    typeof value.content_digest === 'string' &&
    'encoded_size' in value &&
    typeof value.encoded_size === 'number'
  );
}

/** Uses the ordinary Context owner; document selection grants no execution permission. */
export function selectDocument(
  node: Node,
  artifact: Artifact,
  kind: Selection['source_kind'],
  processManaged = false,
): Node {
  const entries = { ...node.context_binding?.entries };
  const existing = documentSelections(node, processManaged).find(
    (item) => item.artifact.artifact_id === artifact.artifact_id,
  );
  const key = existing?.key ?? `project-document.${artifact.artifact_id}`;
  entries[key] = existing
    ? { ...existing.selection, source_kind: kind }
    : {
        source_kind: kind,
        source: { value: { mapping: 'constant', value: { kind: 'artifact', value: artifact } } },
      };
  return { ...node, context_binding: { ...node.context_binding, entries } };
}

export function removeDocument(node: Node, key: string): Node {
  const entries = { ...node.context_binding?.entries };
  delete entries[key];
  const binding = node.context_binding;
  const empty =
    !Object.keys(entries).length &&
    !Object.keys(binding?.artifact_collections ?? {}).length &&
    !binding?.candidate_revision;
  return { ...node, context_binding: empty ? null : { ...binding, entries } };
}

/** Changes only the chosen binding, preserving aliases, guards and exact content identity. */
export function setDocumentRole(node: Node, key: string, kind: 'policy' | 'specification'): Node {
  const selection = node.context_binding?.entries[key];
  if (!selection) return node;
  return {
    ...node,
    context_binding: {
      ...node.context_binding,
      entries: {
        ...node.context_binding?.entries,
        [key]: { ...selection, source_kind: kind },
      },
    },
  };
}
