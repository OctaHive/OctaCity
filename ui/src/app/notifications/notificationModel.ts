import { OPERATOR_ATTENTION_CONSTRAINTS } from '../../../.generated/api/constraints';
import type { OperatorAttentionItem, OperatorAttentionScope } from '../../api/operatorAttention';
import type { PreferenceIdentity } from '../presentation/preferences';
import type { SessionActionOutcome } from './NotificationProvider';

export const MAX_ATTENTION_CENTER_PAGES = 2;
export const MAX_ATTENTION_CENTER_ITEMS =
  OPERATOR_ATTENTION_CONSTRAINTS.defaultPageSize * MAX_ATTENTION_CENTER_PAGES;

/** Converts bounded local favorites into the exact server attention scope. */
export function attentionScope(favorites: readonly PreferenceIdentity[]): OperatorAttentionScope {
  const targets = favorites
    .filter(
      (favorite): favorite is PreferenceIdentity & { kind: 'agent' | 'agent_pool' | 'build' } =>
        favorite.kind === 'agent' || favorite.kind === 'agent_pool' || favorite.kind === 'build',
    )
    .slice(0, OPERATOR_ATTENTION_CONSTRAINTS.maximumTargets);
  return {
    agentIds: sortedIds(targets, 'agent'),
    buildIds: sortedIds(targets, 'build'),
    includeCriticalConditions: true,
    poolIds: sortedIds(targets, 'agent_pool'),
  };
}

/** Defensively keeps only requested favorite targets and target-free critical conditions. */
export function relevantAttentionItems(
  items: readonly OperatorAttentionItem[],
  scope: OperatorAttentionScope,
): readonly OperatorAttentionItem[] {
  const selected = new Set([
    ...scope.agentIds.map((id) => `agent:${id}`),
    ...scope.buildIds.map((id) => `build:${id}`),
    ...scope.poolIds.map((id) => `agent_pool:${id}`),
  ]);
  const seen = new Set<string>();
  return items
    .filter((item) => {
      if (seen.has(item.id)) return false;
      const relevant =
        item.category === 'critical_system'
          ? item.target === null
          : item.target !== null &&
            item.category === item.target.kind &&
            selected.has(`${item.target.kind}:${item.target.id}`);
      if (relevant) seen.add(item.id);
      return relevant;
    })
    .slice(0, MAX_ATTENTION_CENTER_ITEMS);
}

/** Counts relevant items newer than the browser-local time at which the center was opened. */
export function countUnseen(
  actions: readonly SessionActionOutcome[],
  attention: readonly OperatorAttentionItem[],
  lastOpenedAt: number | null,
): number {
  if (lastOpenedAt === null) return actions.length + attention.length;
  return (
    actions.filter((item) => item.occurredAtUnixMs > lastOpenedAt).length +
    attention.filter((item) => latestAttentionTime(item) > lastOpenedAt).length
  );
}

export function latestAttentionTime(item: OperatorAttentionItem): number {
  return Math.max(item.occurred_at_unix_ms, item.resolved_at_unix_ms ?? 0);
}

function sortedIds(
  targets: readonly (PreferenceIdentity & { kind: 'agent' | 'agent_pool' | 'build' })[],
  kind: 'agent' | 'agent_pool' | 'build',
): readonly string[] {
  return targets
    .filter((target) => target.kind === kind)
    .map((target) => target.id)
    .sort();
}
