import type { MessageKey } from '../../app/presentation/messages';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
type Choice = components['schemas']['IssuePropertyChoice'];

/** Legacy numeric levels remain readable alongside exact Project choice IDs. */
const ISSUE_PRIORITY_LEVELS = [
  { label: 'Blocker', value: 3 },
  { label: 'High', value: 2 },
  { label: 'Medium', value: 1 },
  { label: 'Low', value: 0 },
] as const;

type Translate = (key: MessageKey, values?: Readonly<Record<string, string | number>>) => string;

export function formatIssuePriority(
  priority: number | string | null | undefined,
  t: Translate,
  choices: readonly Choice[] = bounds.defaultPriorities,
): string {
  if (priority == null) return t('issues.notSet');
  if (typeof priority === 'string') return choices.find((c) => c.id === priority)?.name ?? priority;
  return (
    ISSUE_PRIORITY_LEVELS.find((level) => level.value === priority)?.label ??
    t('issues.legacyPriority', { value: priority })
  );
}

/** Include an existing non-preset value so editing never silently selects another level. */
export function issuePriorityOptions(
  priority: number | string | null | undefined,
  t: Translate,
  choices: readonly Choice[] = bounds.defaultPriorities,
) {
  // Menu identity must distinguish Project choice "2" from legacy numeric 2.
  const options: { value: string; label: string; priority: number | string | null }[] = [
    { value: '', label: t('issues.notSet'), priority: null },
    ...choices
      .filter((choice) => !choice.retired || choice.id === priority)
      .map((choice) => ({
        value: `choice:${choice.id}`,
        label: choice.retired
          ? t('classification.retiredChoice', { name: choice.name })
          : choice.name,
        priority: choice.id,
      })),
  ];
  if (priority != null && !options.some((option) => option.priority === priority)) {
    options.push({
      value: `${typeof priority === 'string' ? 'choice' : 'legacy'}:${priority}`,
      label: formatIssuePriority(priority, t),
      priority,
    });
  }
  return options;
}
