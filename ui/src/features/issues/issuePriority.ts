import type { MessageKey } from '../../app/presentation/messages';

/** Console planning levels; the optional numeric REST field remains unchanged. */
const ISSUE_PRIORITY_LEVELS = [
  { label: 'Blocker', value: 3 },
  { label: 'High', value: 2 },
  { label: 'Medium', value: 1 },
  { label: 'Low', value: 0 },
] as const;

type Translate = (key: MessageKey, values?: Readonly<Record<string, string | number>>) => string;

export function formatIssuePriority(priority: number | null | undefined, t: Translate): string {
  if (priority == null) return t('issues.notSet');
  return (
    ISSUE_PRIORITY_LEVELS.find((level) => level.value === priority)?.label ??
    t('issues.legacyPriority', { value: priority })
  );
}

/** Include an existing non-preset value so editing never silently selects another level. */
export function issuePriorityOptions(priority: number | null | undefined, t: Translate) {
  const options = [
    { value: '', label: t('issues.notSet') },
    ...ISSUE_PRIORITY_LEVELS.map((level) => ({ value: String(level.value), label: level.label })),
  ];
  if (priority != null && !options.some((option) => option.value === String(priority))) {
    options.push({ value: String(priority), label: formatIssuePriority(priority, t) });
  }
  return options;
}
