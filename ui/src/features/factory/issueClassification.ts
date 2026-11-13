import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type S = components['schemas'];
export const classificationAction = 'apply_issue_classification';

export type ClassificationChoices = S['FactoryIssueClassificationChoices'];

type ClassificationProducer = {
  node: string;
  outcome: Pick<S['FactoryFlowNodeDefinition']['outcomes'][number], 'key' | 'schema'>;
};

/** An equal schema alone does not identify the accepted result being compiled. */
export function sameClassificationProducer(
  current: ClassificationProducer | undefined,
  captured: ClassificationProducer,
) {
  return (
    !!current &&
    current.node === captured.node &&
    current.outcome.key === captured.outcome.key &&
    sourceReferenceKey(current.outcome.schema) === sourceReferenceKey(captured.outcome.schema)
  );
}

/** Preserve optional fields, including declared assessment dependencies, without inventing values. */
export function assessmentMapping(
  fields: Extract<S['FactoryFlowValueSchema'], { type: 'object' }>['fields'],
  alias = 'assessment',
): S['FactoryFlowValueMapping'] {
  const selected = Object.entries(fields);
  const maximum = FACTORY_AUTHORING_SCHEMAS.FactoryFlowDataMapping['x-max-encoded-bytes'];
  const encoded = (value: unknown) => new TextEncoder().encode(JSON.stringify(value)).length;
  const tooLarge = () => new RangeError('Issue projection exceeds the input mapping limits');
  if (
    selected.filter(([, field]) => !field.required).length + 1 >
    FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueMapping['x-max-depth']
  )
    throw tooLarge();
  let bytes = encoded({ value: null }) - encoded(null);
  const reserve = (amount: number) => {
    bytes += amount;
    if (bytes > maximum) throw tooLarge();
  };
  const build = (
    index: number,
    projection: Record<string, S['FactoryFlowValueMapping']>,
  ): S['FactoryFlowValueMapping'] => {
    const field = selected[index];
    if (!field) {
      const value = { mapping: 'object' as const, fields: projection };
      reserve(encoded(value));
      return value;
    }
    const [name, schema] = field;
    const path = `/${alias}/${name}`;
    const present = build(index + 1, { ...projection, [name]: { mapping: 'pointer', path } });
    if (schema.required) return present;
    reserve(
      encoded({ mapping: 'if_present', path, then: null, otherwise: null }) - 2 * encoded(null),
    );
    return { mapping: 'if_present', path, then: present, otherwise: build(index + 1, projection) };
  };
  return build(0, {});
}

export function classificationParameters(
  action: S['FactoryFlowNodeDefinition']['action'],
): Record<string, unknown> | null {
  return action?.parameters &&
    typeof action.parameters === 'object' &&
    !Array.isArray(action.parameters)
    ? (action.parameters as Record<string, unknown>)
    : null;
}

export function matchesClassification(
  action: NonNullable<S['FactoryFlowNodeDefinition']['action']>,
  choices: ClassificationChoices,
  fields?: S['FactoryIssueFieldChoices'],
): boolean {
  const parameters = action.parameters;
  if (
    action.plugin.identity !== choices.plugin.identity ||
    action.plugin.version !== choices.plugin.version ||
    action.plugin.digest !== choices.plugin.digest ||
    !parameters ||
    typeof parameters !== 'object' ||
    !('policy' in parameters) ||
    parameters.policy !== choices.policy
  )
    return false;
  if ('fields' in parameters) {
    const available = fields?.classification;
    const selected = parameters.fields;
    if (
      !available ||
      !selected ||
      typeof selected !== 'object' ||
      Array.isArray(selected) ||
      'size_labels' in parameters
    )
      return false;
    const values = Object.entries(selected);
    const booleans = Object.entries(available).filter(([, value]) => typeof value === 'boolean');
    if (
      values.length !== booleans.length + 2 ||
      !('definition_version' in selected) ||
      selected.definition_version !== fields.definition_version ||
      !('properties' in selected) ||
      !selected.properties ||
      typeof selected.properties !== 'object' ||
      Array.isArray(selected.properties)
    )
      return false;
    const entries = Object.entries(selected.properties);
    return (
      booleans.every(([key, supported]) =>
        values.some(
          ([name, value]) => name === key && typeof value === 'boolean' && (!value || supported),
        ),
      ) &&
      entries.every(([slot, id]) =>
        Object.entries(available.properties).some(
          ([name, target]) => name === slot && target === id,
        ),
      ) &&
      (entries.length > 0 || values.some(([, value]) => value === true))
    );
  }
  if (
    !('size_labels' in parameters) ||
    !parameters.size_labels ||
    typeof parameters.size_labels !== 'object'
  )
    return false;
  const labels = parameters.size_labels as Record<string, unknown>;
  return (
    Object.keys(labels).length === choices.size_labels.length &&
    choices.size_labels.every(({ size, label }) => labels[size] === label)
  );
}

/** Remove the optional write only when the remaining graph can keep its data dependencies. */
export function withoutClassification(
  root: S['FactoryFlowRootDraft'],
  node: S['FactoryFlowNodeDefinition'],
): S['FactoryFlowRootDraft'] | null {
  const key = node.key;
  const incoming = root.transitions.filter(
    (edge) => edge.target.kind === 'node' && edge.target.key === key,
  );
  const outgoing = root.transitions.filter((edge) => edge.predecessor === key);
  const [entry] = incoming;
  const [exit] = outgoing;
  if (
    root.entry === key ||
    incoming.length !== 1 ||
    !entry ||
    entry.predecessor === key ||
    entry.max_repeats !== 0 ||
    outgoing.length > 1 ||
    (exit && (exit.max_repeats !== 0 || exit.target.key === key)) ||
    node.outcomes.length !== 1 ||
    (exit && exit.outcome !== node.outcomes[0]?.key) ||
    root.context_projections.some((edge) => edge.predecessor === key || edge.successor === key) ||
    root.data_projections.some((edge) => edge.predecessor === key || edge.successor === key) ||
    root.nodes.some((step) =>
      Object.values(step.input_binding?.sources ?? {}).some(
        (source) =>
          (source.source === 'record' && source.node === key) ||
          (exit?.target.kind === 'node' &&
            step.key === exit.target.key &&
            (source.source === 'predecessor' || source.source === 'incoming')),
      ),
    )
  )
    return null;
  let terminals = root.terminals;
  if (exit?.target.kind === 'terminal') {
    const terminal = exit.target.key;
    const source = root.nodes
      .find((step) => step.key === entry.predecessor)
      ?.outcomes.find((outcome) => outcome.key === entry.outcome);
    if (
      !source ||
      root.transitions.some(
        (edge) => edge !== exit && edge.target.kind === 'terminal' && edge.target.key === terminal,
      )
    )
      return null;
    terminals = terminals.map((item) =>
      item.key === terminal ? { ...item, schema: source.schema } : item,
    );
  }
  return {
    ...root,
    nodes: root.nodes.filter((step) => step.key !== key),
    transitions: root.transitions.flatMap((edge) =>
      edge === entry
        ? exit
          ? [{ ...entry, target: exit.target }]
          : []
        : edge === exit
          ? []
          : [edge],
    ),
    terminals,
  };
}
