import type { components } from '../../../.generated/api/schema';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { referencesFlowSchema, registerFlowSchemas } from './flowSchemas';

type S = components['schemas'];
type Draft = S['FactoryFlowDefinitionDraft'];
type Reference = S['FactoryImmutableReferenceResource'];

/** A selected command resolves one shared report slot, including downstream gates and terminals. */
function replaceSchema(draft: Draft, from: Reference, to: Reference): Draft {
  const replace = (value: Reference) =>
    sourceReferenceKey(value) === sourceReferenceKey(from) ? to : value;
  return {
    ...draft,
    root: {
      ...draft.root,
      input_schema: draft.root.input_schema ? replace(draft.root.input_schema) : null,
      nodes: draft.root.nodes.map((node) => ({
        ...node,
        input_schema: node.input_schema ? replace(node.input_schema) : null,
        outcomes: node.outcomes.map((outcome) => ({ ...outcome, schema: replace(outcome.schema) })),
        ...(node.input_binding
          ? {
              input_binding: {
                ...node.input_binding,
                sources: Object.fromEntries(
                  Object.entries(node.input_binding.sources).map(([name, source]) => [
                    name,
                    source.source === 'incoming'
                      ? { ...source, schema: replace(source.schema) }
                      : source,
                  ]),
                ),
              },
            }
          : {}),
      })),
      data_projections: draft.root.data_projections.map((projection) => ({
        ...projection,
        schema: replace(projection.schema),
      })),
      terminals: draft.root.terminals.map((terminal) => ({
        ...terminal,
        schema: replace(terminal.schema),
      })),
    },
  };
}

/** Draft-only defaults never grant execution and disappear when their ordinary bindings are chosen. */
export function resolveDraftBindings(previous: Draft, proposed: Draft): Draft {
  let next = proposed;
  const captures = { ...next.execution_defaults };
  const pending = new Set((next.schema_slots ?? []).map(sourceReferenceKey));
  const replaced: Reference[] = [];
  for (const node of next.root.nodes) {
    const before = previous.root.nodes.find((item) => item.key === node.key);
    if (!node.build && !node.profile_selection) continue;
    for (const outcome of node.outcomes) {
      const prior = before?.outcomes.find((item) => item.key === outcome.key)?.schema;
      if (prior && pending.delete(sourceReferenceKey(prior))) {
        next = replaceSchema(next, prior, outcome.schema);
        replaced.push(prior);
      }
    }
  }
  next = {
    ...next,
    root: {
      ...next.root,
      nodes: next.root.nodes.map((node) => {
        const defaults = captures[node.key];
        if (!defaults || (!node.build && !node.profile_selection)) return node;
        delete captures[node.key];
        const patch = {
          ...(defaults.capture_write_paths
            ? { capture_write_paths: defaults.capture_write_paths }
            : {}),
          ...(defaults.capture_deliverables
            ? { capture_deliverables: defaults.capture_deliverables }
            : {}),
        };
        return {
          ...node,
          ...(node.build ? { build: { ...node.build, ...patch } } : {}),
          ...(node.profile_selection
            ? {
                profile_selection: {
                  ...node.profile_selection,
                  candidates: Object.fromEntries(
                    Object.entries(node.profile_selection.candidates).map(([key, candidate]) => [
                      key,
                      { ...candidate, build: { ...candidate.build, ...patch } },
                    ]),
                  ),
                },
              }
            : {}),
        };
      }),
    },
  };
  for (const key of Object.keys(captures)) {
    const node = next.root.nodes.find((item) => item.key === key);
    if (!node || (node.kind !== 'reasoning' && node.kind !== 'build_command')) delete captures[key];
  }
  return registerFlowSchemas(
    {
      ...next,
      ...(previous.execution_defaults || proposed.execution_defaults
        ? { execution_defaults: captures }
        : {}),
      ...(previous.schema_slots || proposed.schema_slots
        ? {
            schema_slots: (next.schema_slots ?? []).filter(
              (ref) => pending.has(sourceReferenceKey(ref)) && referencesFlowSchema(next.root, ref),
            ),
          }
        : {}),
    },
    [],
    replaced,
  );
}
