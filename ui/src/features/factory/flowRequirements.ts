import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { classificationAction } from './issueClassification';
import { publicationChoice, publishCandidateAction } from './PublicationStep';
type S = components['schemas'];
type Node = S['FactoryFlowNodeDefinition'];
type Budget = S['FactoryBudget'];
type Permissions = S['FactoryPermissions'];
const containsPath = (parent: string, child: string) =>
  parent === child || parent === '/' || child.startsWith(`${parent}/`);
const coversMount = (
  allowed: Permissions['mounts'][number],
  requested: Permissions['mounts'][number],
) =>
  containsPath(allowed.root, requested.root) &&
  (allowed.mode === 'read_write' || requested.mode === 'read_only');
const coversCommand = (
  allowed: Permissions['commands'][number],
  requested: Permissions['commands'][number],
) =>
  allowed.executable.identity === requested.executable.identity &&
  allowed.executable.version === requested.executable.version &&
  allowed.executable.digest === requested.executable.digest &&
  allowed.arguments.length === requested.arguments.length &&
  allowed.arguments.every(
    (argument, index) =>
      argument.kind === 'any' ||
      (requested.arguments[index]?.kind === 'exact' &&
        argument.value === requested.arguments[index]?.value),
  );
export const budgetKeys = [
  'max_attempts',
  'max_elapsed_millis',
  'max_tokens',
  'max_cost_micro_units',
  'max_output_bytes',
] as const;

/** Alternatives use the largest requirement, plus selection; they are not parallel executions. */
export function profileBudget(node: Node): Budget | null {
  const selection = node.profile_selection;
  const profiles = Object.values(selection?.candidates ?? {});
  if (!profiles.length || !selection) return null;
  const usage = selection.selection_budget;
  if (typeof usage.cost_micro_units !== 'number') return null;
  const overhead = [
    usage.attempts,
    usage.elapsed_millis,
    usage.tokens,
    usage.cost_micro_units,
    usage.output_bytes,
  ];
  return Object.fromEntries(
    budgetKeys.map((key, index) => [
      key,
      Math.max(...profiles.map(({ profile }) => profile.definition.budget[key])) + overhead[index]!,
    ]),
  ) as Budget;
}

/** Conservative bound across all declared branches, technical retries and bounded back edges.
 * Subflow budgets require their published closure and deliberately stay manual here. */
export function flowBudget(root: S['FactoryFlowRootDraft']): Budget | null {
  if (!root.nodes.length || root.nodes.some((node) => node.kind === 'subflow_call')) return null;
  const repeats = root.transitions.reduce((bound, edge) => bound * (1 + edge.max_repeats), 1);
  const values = budgetKeys.map((key) =>
    Math.max(
      1,
      root.nodes.reduce(
        (sum, node) => sum + node.budget[key] * (1 + (node.max_execution_retries ?? 0)),
        0,
      ) * repeats,
    ),
  );
  return values.every(Number.isSafeInteger)
    ? (Object.fromEntries(budgetKeys.map((key, index) => [key, values[index]!])) as Budget)
    : null;
}

/** Both per-outcome route counts and structural successors constrain fan-out. */
export function flatGraphLimits(root: S['FactoryFlowRootDraft']) {
  if (root.nodes.some((node) => node.kind === 'subflow_call')) return null;
  const routes = new Map<string, number>(),
    successors = new Map<string, Set<string>>();
  for (const edge of root.transitions) {
    const key = `${edge.predecessor}:${edge.outcome}`;
    routes.set(key, (routes.get(key) ?? 0) + 1);
    if (edge.target.kind === 'node') {
      const targets = successors.get(edge.predecessor) ?? new Set<string>();
      targets.add(edge.target.key);
      successors.set(edge.predecessor, targets);
    }
  }
  const minimum = FACTORY_AUTHORING_SCHEMAS.FactoryFlowAdmissionLimits.properties;
  return {
    max_depth: minimum.max_depth.minimum,
    max_expanded_nodes: Math.max(minimum.max_expanded_nodes.minimum, root.nodes.length),
    max_fan_out: Math.max(
      minimum.max_fan_out.minimum,
      ...routes.values(),
      ...[...successors.values()].map((targets) => targets.size),
    ),
    max_repeat_count: Math.max(
      minimum.max_repeat_count.minimum,
      ...root.transitions.map((edge) => edge.max_repeats),
    ),
    max_wip: Math.max(minimum.max_wip.minimum, root.execution.max_active_nodes),
  };
}

/** Merge declared requests without dropping existing grants. This is draft intent,
 * never an effective authorization; Project/agent/backend policy still applies. */
export function mergePermissions(left: Permissions, right: Permissions): Permissions {
  const union = <T>(a: T[], b: T[]) => [
    ...new Map([...a, ...b].map((item) => [JSON.stringify(item), item])).values(),
  ];
  const refs = (a: Permissions['plugins'], b: Permissions['plugins']) => [
    ...new Map(
      [...a, ...b].map((item) => [`${item.identity}:${item.version}:${item.digest}`, item]),
    ).values(),
  ];
  let mounts = [...left.mounts];
  for (const mount of right.mounts) {
    if (mounts.some((allowed) => coversMount(allowed, mount))) continue;
    mounts = [...mounts.filter((existing) => !coversMount(mount, existing)), mount];
  }
  let commands = [...left.commands];
  for (const command of right.commands) {
    if (commands.some((allowed) => coversCommand(allowed, command))) continue;
    commands = [...commands.filter((existing) => !coversCommand(command, existing)), command];
  }
  return {
    ...left,
    plugins: refs(left.plugins, right.plugins),
    executables: refs(left.executables, right.executables),
    tools: refs(left.tools, right.tools),
    commands,
    mounts,
    network_hosts: union(left.network_hosts, right.network_hosts),
    secret_profiles: union(left.secret_profiles, right.secret_profiles),
    workload_identity_profiles: union(
      left.workload_identity_profiles,
      right.workload_identity_profiles,
    ),
    max_descendants: Math.max(left.max_descendants, right.max_descendants),
    resources: {
      ...left.resources,
      ...Object.fromEntries(
        Object.keys(left.resources).map((key) => [
          key,
          Math.max(
            left.resources[key as keyof Permissions['resources']],
            right.resources[key as keyof Permissions['resources']],
          ),
        ]),
      ),
    },
    outputs: {
      ...left.outputs,
      kinds: union(left.outputs.kinds, right.outputs.kinds),
      max_artifact_bytes: Math.max(
        left.outputs.max_artifact_bytes,
        right.outputs.max_artifact_bytes,
      ),
      max_artifact_count: Math.max(
        left.outputs.max_artifact_count,
        right.outputs.max_artifact_count,
      ),
      max_report_bytes: Math.max(left.outputs.max_report_bytes, right.outputs.max_report_bytes),
      max_report_count: Math.max(left.outputs.max_report_count, right.outputs.max_report_count),
    },
  };
}
export function requiredPermissions(
  node: Node,
  publications: readonly S['FactoryCandidatePublicationChoice'][] = [],
): Permissions {
  const action = node.action;
  const target = publicationChoice(action, publications);
  const base =
    action?.action === classificationAction || action?.action === publishCandidateAction
      ? mergePermissions(
          node.permissions,
          target?.permissions ?? { ...node.permissions, plugins: [action.plugin] },
        )
      : node.permissions;
  return Object.values(node.profile_selection?.candidates ?? {}).reduce(
    (permissions, { profile }) => mergePermissions(permissions, profile.definition.permissions),
    base,
  );
}

/** Overlapping roots with different access cannot be represented without
 * granting more than the union. Leave this choice to explicit manual review. */
export function conflictingMounts(permissions: Permissions) {
  return permissions.mounts.some((mount, index) =>
    permissions.mounts
      .slice(index + 1)
      .some(
        (other) => containsPath(mount.root, other.root) || containsPath(other.root, mount.root),
      ),
  );
}
