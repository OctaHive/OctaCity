import type { MessageKey } from '../../app/presentation/messages';

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
): string {
  if (priority == null) return t('issues.notSet');
  if (typeof priority === 'string') return priority;
  return (
    ISSUE_PRIORITY_LEVELS.find((level) => level.value === priority)?.label ??
    t('issues.legacyPriority', { value: priority })
  );
}

/** Include an existing non-preset value so editing never silently selects another level. */
export function issuePriorityOptions(priority: number | string | null | undefined, t: Translate) {
  // Menu identity must distinguish Project choice "2" from legacy numeric 2.
  const options: { value: string; label: string; priority: number | string | null }[] = [
    { value: '', label: t('issues.notSet'), priority: null },
    ...ISSUE_PRIORITY_LEVELS.map((level) => ({
      value: `legacy:${level.value}`,
      label: level.label,
      priority: level.value,
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
