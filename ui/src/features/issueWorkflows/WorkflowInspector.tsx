import { useId, type Ref } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowCanvas.module.css';
import { ISSUE_WORKFLOW_CONSTRAINTS as bounds } from '../../../.generated/api/issue-workflow';
import {
  WorkflowConnectionFields,
  WorkflowTransitionFields,
  type WorkflowConnection,
} from './WorkflowTransitionFields';
import viewStyles from './WorkflowView.module.css';

export type WorkflowSelection = { kind: 'status' | 'transition'; id: string };

export function WorkflowInspector({
  definition,
  selection,
  onChange,
  onRemoveStatus,
  onRemoveTransition,
  connection,
  inspectorRef,
  inspectorId,
  onReturnToCanvas,
}: {
  inspectorRef?: Ref<HTMLElement> | undefined;
  inspectorId?: string | undefined;
  onReturnToCanvas?: (() => void) | undefined;
  definition: WorkflowDefinition;
  selection: WorkflowSelection | null;
  onChange?: ((definition: WorkflowDefinition) => void) | undefined;
  connection?: WorkflowConnection | undefined;
  onRemoveTransition?: ((id: string) => void) | undefined;
  onRemoveStatus?: ((id: string) => void) | undefined;
}) {
  const { t } = usePresentation();
  const initialGroupId = useId();
  const status =
    selection?.kind === 'status'
      ? definition.statuses.find((item) => item.id === selection.id)
      : undefined;
  const transition =
    selection?.kind === 'transition'
      ? definition.transitions.find((item) => item.id === selection.id)
      : undefined;
  const statusName = (id: string) =>
    definition.statuses.find((item) => item.id === id)?.name ?? t('workflow.unnamedStatus');
  const fields = status
    ? [
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
          [t('workflow.from'), statusName(transition.from)],
          [t('workflow.to'), statusName(transition.to)],
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
    <aside
      ref={inspectorRef}
      id={inspectorId}
      tabIndex={-1}
      className={styles.inspector}
      aria-label={t('workflow.properties')}
    >
      {onChange ? (
        <label>
          {t('workflow.name')}
          <input
            value={definition.name}
            maxLength={bounds.nameMaximumBytes}
            onChange={(event) => onChange({ ...definition, name: event.target.value })}
          />
        </label>
      ) : (
        <p>{t('workflow.readOnly')}</p>
      )}
      {onChange && status ? (
        <>
          <label>
            {t('workflow.statusName')}
            <input
              value={status.name}
              maxLength={bounds.nameMaximumBytes}
              onChange={(event) =>
                onChange({
                  ...definition,
                  statuses: definition.statuses.map((item) =>
                    item.id === status.id ? { ...item, name: event.target.value } : item,
                  ),
                })
              }
            />
          </label>
          <label className={viewStyles.choice}>
            <input
              type="radio"
              name={initialGroupId}
              checked={definition.initial_status_id === status.id}
              onChange={() => onChange({ ...definition, initial_status_id: status.id })}
            />
            {t('workflow.initial')}
          </label>
          <label className={viewStyles.choice}>
            <input
              type="checkbox"
              checked={definition.terminal_status_ids.includes(status.id)}
              onChange={(event) =>
                onChange({
                  ...definition,
                  terminal_status_ids: event.target.checked
                    ? [...definition.terminal_status_ids, status.id]
                    : definition.terminal_status_ids.filter((id) => id !== status.id),
                })
              }
            />
            {t('workflow.terminal')}
          </label>
          {onRemoveStatus ? (
            <button type="button" onClick={() => onRemoveStatus(status.id)}>
              {t('workflow.removeStatus')}
            </button>
          ) : null}
        </>
      ) : null}
      {onChange && transition ? (
        <WorkflowTransitionFields
          draft={definition}
          edge={transition}
          onChange={onChange}
          onRemove={onRemoveTransition}
        />
      ) : null}
      {!onChange && (status || transition) ? (
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
      ) : null}
      {connection ? (
        <WorkflowConnectionFields
          key={connection.from}
          draft={definition}
          connection={connection}
        />
      ) : null}
      {!connection && !status && !transition ? <p>{t('workflow.selectElement')}</p> : null}
      {onReturnToCanvas ? (
        <button type="button" onClick={onReturnToCanvas}>
          {t('workflow.backToCanvas')}
        </button>
      ) : null}
    </aside>
  );
}
