import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowCanvas.module.css';

export type WorkflowSelection = { kind: 'status' | 'transition'; id: string };

export function WorkflowInspector({
  definition,
  selection,
}: {
  definition: WorkflowDefinition;
  selection: WorkflowSelection | null;
}) {
  const { t } = usePresentation();
  const status =
    selection?.kind === 'status'
      ? definition.statuses.find((item) => item.id === selection.id)
      : undefined;
  const transition =
    selection?.kind === 'transition'
      ? definition.transitions.find((item) => item.id === selection.id)
      : undefined;
  const endpoint = (id: string) =>
    `${definition.statuses.find((item) => item.id === id)?.name ?? id} · ${id}`;
  const fields = status
    ? [
        [t('workflow.statusId'), status.id],
        [t('workflow.order'), String(definition.statuses.indexOf(status) + 1)],
        [
          t('workflow.initial'),
          t(definition.initial_status_id === status.id ? 'common.yes' : 'common.no'),
        ],
        [
          t('workflow.terminal'),
          t(definition.terminal_status_ids.includes(status.id) ? 'common.yes' : 'common.no'),
        ],
      ]
    : transition
      ? [
          [t('workflow.transitionId'), transition.id],
          [t('workflow.from'), endpoint(transition.from)],
          [t('workflow.to'), endpoint(transition.to)],
          [
            t('workflow.origins'),
            transition.origins.map((origin) => t(`workflow.origin.${origin}`)).join(', ') ||
              t('workflow.none'),
          ],
          [
            t('workflow.evidence'),
            transition.evidence.map((item) => t(`workflow.evidence.${item}`)).join(', ') ||
              t('workflow.none'),
          ],
        ]
      : [];
  return (
    <aside className={styles.inspector} aria-label={t('workflow.properties')}>
      <p>{t('workflow.readOnly')}</p>
      {status || transition ? (
        <>
          <h4>{status?.name ?? transition?.name}</h4>
          <dl>
            {fields.map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
        </>
      ) : (
        <p>{t('workflow.selectElement')}</p>
      )}
    </aside>
  );
}
