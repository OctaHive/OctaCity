import { OPERATOR_ATTENTION_CONSTRAINTS } from '../../../.generated/api/constraints';
import { describe, expect, it } from 'vitest';

import type { OperatorAttentionItem } from '../../api/operatorAttention';
import type { PreferenceIdentity } from '../presentation/preferences';
import {
  attentionScope,
  countUnseen,
  MAX_ATTENTION_CENTER_ITEMS,
  relevantAttentionItems,
} from './notificationModel';

describe('notification model', () => {
  it('bounds and sorts favorite targets while excluding unsupported resource kinds', () => {
    const favorites: PreferenceIdentity[] = [
      { id: uuid(999), kind: 'project' },
      ...Array.from(
        { length: OPERATOR_ATTENTION_CONSTRAINTS.maximumTargets + 5 },
        (_, index): PreferenceIdentity => ({ id: uuid(index), kind: 'build' }),
      ),
    ];

    const scope = attentionScope(favorites);

    expect(scope.includeCriticalConditions).toBe(true);
    expect(scope.buildIds).toHaveLength(OPERATOR_ATTENTION_CONSTRAINTS.maximumTargets);
    expect(scope.buildIds).toEqual([...scope.buildIds].sort());
    expect(scope.agentIds).toEqual([]);
    expect(scope.poolIds).toEqual([]);
  });

  it('deduplicates, filters unrelated targets, and bounds rendered attention', () => {
    const favorite = uuid(1);
    const scope = attentionScope([{ id: favorite, kind: 'build' }]);
    const items = [
      targetItem('favorite', favorite),
      targetItem('favorite', favorite),
      targetItem('unrelated', uuid(2)),
      ...Array.from({ length: MAX_ATTENTION_CENTER_ITEMS + 5 }, (_, index) =>
        criticalItem(`critical-${index}`, index),
      ),
    ];

    const relevant = relevantAttentionItems(items, scope);

    expect(relevant).toHaveLength(MAX_ATTENTION_CENTER_ITEMS);
    expect(relevant.filter((item) => item.id === 'favorite')).toHaveLength(1);
    expect(relevant.some((item) => item.id === 'unrelated')).toBe(false);
  });

  it('uses a resolution transition when computing browser-local unseen state', () => {
    const item = criticalItem('resolved', 100, 300);

    expect(countUnseen([], [item], 200)).toBe(1);
    expect(countUnseen([], [item], 300)).toBe(0);
  });
});

function targetItem(id: string, targetId: string): OperatorAttentionItem {
  return {
    category: 'build',
    code: 'build_failed',
    id,
    occurred_at_unix_ms: 100,
    resolved_at_unix_ms: null,
    severity: 'warning',
    summary: id,
    target: { id: targetId, kind: 'build' },
  };
}

function criticalItem(
  id: string,
  occurredAt: number,
  resolvedAt: number | null = null,
): OperatorAttentionItem {
  return {
    category: 'critical_system',
    code: 'required_dependency_unavailable',
    id,
    occurred_at_unix_ms: occurredAt,
    resolved_at_unix_ms: resolvedAt,
    severity: 'critical',
    summary: id,
    target: null,
  };
}

function uuid(index: number): string {
  return `11111111-1111-4111-8111-${index.toString().padStart(12, '0')}`;
}
