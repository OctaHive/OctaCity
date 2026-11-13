import { replaceEqualDeep } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { assessmentMapping } from './issueClassification';

type S = components['schemas'];

/** Recognize only inputs whose shapes, paths and mapping the snapshot form can round-trip. */
export function editableSnapshotFields(
  node: S['FactoryFlowNodeDefinition'],
  snapshot: S['FactoryFlowDefinitionDraft']['issue_snapshot'],
  schemas: readonly S['FactoryFlowDataSchema'][],
): string[] | null {
  const binding = node.input_binding;
  const sources = Object.entries(binding?.sources ?? {});
  const entry = sources[0];
  if (!binding || !snapshot || sources.length !== 1 || entry?.[0] !== 'issue') return null;
  const source = entry[1];
  if (
    source.source !== 'incoming' ||
    source.path !== '' ||
    sourceReferenceKey(source.schema) !== sourceReferenceKey(snapshot.schema)
  )
    return null;
  const input = schemas.find(
    (schema) => sourceReferenceKey(schema.reference) === sourceReferenceKey(snapshot.schema),
  )?.shape;
  const target =
    node.input_schema &&
    schemas.find(
      (schema) => sourceReferenceKey(schema.reference) === sourceReferenceKey(node.input_schema!),
    )?.shape;
  if (input?.type !== 'object' || target?.type !== 'object') return null;
  const names = Object.keys(target.fields);
  if (
    !names.every(
      (name) =>
        !!input.fields[name] &&
        replaceEqualDeep(target.fields[name], input.fields[name]) === target.fields[name],
    )
  )
    return null;
  if (
    names.length === Object.keys(input.fields).length &&
    binding.mapping.value.mapping === 'pointer' &&
    binding.mapping.value.path === '/issue'
  )
    return names;
  try {
    // The server sorts schema fields; the generated presence guards retain click order.
    // Recover that order from the all-present branch, then verify the entire mapping.
    const ordered = Object.fromEntries(
      names
        .filter((name) => target.fields[name]!.required)
        .map((name) => [name, target.fields[name]!]),
    );
    let branch = binding.mapping.value;
    while (branch.mapping === 'if_present') {
      const name = branch.path.slice('/issue/'.length);
      const field = target.fields[name];
      if (
        branch.path !== `/issue/${name}` ||
        !field ||
        field.required ||
        Object.hasOwn(ordered, name)
      )
        return null;
      ordered[name] = field;
      branch = branch.then;
    }
    if (Object.keys(ordered).length !== names.length) return null;
    const expected = assessmentMapping(ordered, 'issue');
    return replaceEqualDeep(binding.mapping.value, expected) === binding.mapping.value
      ? names
      : null;
  } catch (failure) {
    if (failure instanceof RangeError) return null;
    throw failure;
  }
}
