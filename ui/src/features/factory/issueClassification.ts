import type { components } from '../../../.generated/api/schema';

type S = components['schemas'];
export const classificationAction = 'apply_issue_classification';

export type ClassificationChoices = S['FactoryIssueClassificationChoices'];

export function matchesClassification(
  action: NonNullable<S['FactoryFlowNodeDefinition']['action']>,
  choices: ClassificationChoices,
): boolean {
  const parameters = action.parameters;
  if (
    action.plugin.identity !== choices.plugin.identity ||
    action.plugin.version !== choices.plugin.version ||
    action.plugin.digest !== choices.plugin.digest ||
    !parameters ||
    typeof parameters !== 'object' ||
    !('policy' in parameters) ||
    parameters.policy !== choices.policy ||
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
