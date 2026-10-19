import { ISSUE_WORKFLOW_CONSTRAINTS as bounds } from '../../../.generated/api/issue-workflow';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowView.module.css';

type Props = { draft: WorkflowDefinition; onChange: (draft: WorkflowDefinition) => void };

/** References use stable IDs; removing a referenced status requires an explicit edit first. */
export function WorkflowFields({ draft, onChange }: Props) {
  const { t } = usePresentation();
  function move(index: number, offset: number) {
    const statuses = [...draft.statuses];
    const [item] = statuses.splice(index, 1);
    if (item) statuses.splice(index + offset, 0, item);
    onChange({ ...draft, statuses });
  }
  const options = draft.statuses.map((item) => ({ value: item.id, label: item.name }));
  return (
    <>
      <label>
        {t('workflow.name')}
        <input
          value={draft.name}
          maxLength={bounds.nameMaximumBytes}
          onChange={(event) => onChange({ ...draft, name: event.target.value })}
        />
      </label>
      <fieldset>
        <legend>{t('workflow.statuses')}</legend>
        <p>{t('workflow.statusHelp')}</p>
        <ol className={styles.rows}>
          {draft.statuses.map((status, index) => (
            <li key={status.id}>
              <label>
                {t('workflow.statusName', { number: index + 1 })}
                <input
                  value={status.name}
                  maxLength={bounds.nameMaximumBytes}
                  onChange={(event) =>
                    onChange({
                      ...draft,
                      statuses: draft.statuses.map((item) =>
                        item.id === status.id ? { ...item, name: event.target.value } : item,
                      ),
                    })
                  }
                />
              </label>
              <div className={styles.actions}>
                <button
                  type="button"
                  aria-label={t('workflow.moveUp', { number: index + 1 })}
                  disabled={index === 0}
                  onClick={() => move(index, -1)}
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={t('workflow.moveDown', { number: index + 1 })}
                  disabled={index === draft.statuses.length - 1}
                  onClick={() => move(index, 1)}
                >
                  ↓
                </button>
                <button
                  type="button"
                  disabled={
                    draft.statuses.length <= bounds.minimumStatuses ||
                    status.id === draft.initial_status_id ||
                    draft.transitions.some(
                      (edge) => edge.from === status.id || edge.to === status.id,
                    )
                  }
                  onClick={() =>
                    onChange({
                      ...draft,
                      statuses: draft.statuses.filter((item) => item.id !== status.id),
                      terminal_status_ids: draft.terminal_status_ids.filter(
                        (id) => id !== status.id,
                      ),
                    })
                  }
                >
                  {t('workflow.removeStatus', { number: index + 1 })}
                </button>
              </div>
            </li>
          ))}
        </ol>
        <button
          type="button"
          disabled={draft.statuses.length >= bounds.maximumStatuses}
          onClick={() =>
            onChange({
              ...draft,
              statuses: [...draft.statuses, { id: crypto.randomUUID(), name: '' }],
            })
          }
        >
          {t('workflow.addStatus')}
        </button>
      </fieldset>
      <div>
        {t('workflow.initial')}
        <SelectMenu
          ariaLabel={t('workflow.initial')}
          value={draft.initial_status_id}
          options={options}
          onValueChange={(initial_status_id) => onChange({ ...draft, initial_status_id })}
        />
      </div>
      <fieldset>
        <legend>{t('workflow.terminals')}</legend>
        <p>{t('workflow.terminalHelp')}</p>
        {draft.statuses.map((status) => (
          <label className={styles.choice} key={status.id}>
            <input
              type="checkbox"
              checked={draft.terminal_status_ids.includes(status.id)}
              onChange={(event) =>
                onChange({
                  ...draft,
                  terminal_status_ids: event.target.checked
                    ? [...draft.terminal_status_ids, status.id]
                    : draft.terminal_status_ids.filter((id) => id !== status.id),
                })
              }
            />
            {status.name}
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t('workflow.transitions')}</legend>
        <p>{t('workflow.transitionHelp')}</p>
        {draft.transitions.map((edge, index) => {
          const update = (patch: Partial<typeof edge>) =>
            onChange({
              ...draft,
              transitions: draft.transitions.map((item) =>
                item.id === edge.id ? { ...item, ...patch } : item,
              ),
            });
          return (
            <fieldset key={edge.id}>
              <legend>{t('workflow.transitionNumber', { number: index + 1 })}</legend>
              <label>
                {t('workflow.transitionName')}
                <input
                  value={edge.name}
                  maxLength={bounds.nameMaximumBytes}
                  onChange={(event) => update({ name: event.target.value })}
                />
              </label>
              <div className={styles.endpoints}>
                <div>
                  {t('workflow.from')}
                  <SelectMenu
                    ariaLabel={t('workflow.from')}
                    options={options}
                    value={edge.from}
                    onValueChange={(from) => update({ from })}
                  />
                </div>
                <div>
                  {t('workflow.to')}
                  <SelectMenu
                    ariaLabel={t('workflow.to')}
                    options={options}
                    value={edge.to}
                    onValueChange={(to) => update({ to })}
                  />
                </div>
              </div>
              <fieldset>
                <legend>{t('workflow.origins')}</legend>
                {bounds.origins.map((origin) => (
                  <label key={origin} className={styles.choice}>
                    <input
                      type="checkbox"
                      checked={edge.origins.includes(origin)}
                      onChange={(event) =>
                        update({
                          origins: event.target.checked
                            ? [...edge.origins, origin]
                            : edge.origins.filter((item) => item !== origin),
                        })
                      }
                    />
                    {t(`workflow.origin.${origin}`)}
                  </label>
                ))}
              </fieldset>
              <fieldset>
                <legend>{t('workflow.evidence')}</legend>
                {bounds.evidence.map((evidence) => (
                  <label key={evidence} className={styles.choice}>
                    <input
                      type="checkbox"
                      checked={edge.evidence.includes(evidence)}
                      onChange={(event) =>
                        update({
                          evidence: event.target.checked
                            ? [...edge.evidence, evidence]
                            : edge.evidence.filter((item) => item !== evidence),
                        })
                      }
                    />
                    {t(`workflow.evidence.${evidence}`)}
                  </label>
                ))}
              </fieldset>
              <button
                type="button"
                onClick={() =>
                  onChange({
                    ...draft,
                    transitions: draft.transitions.filter((item) => item.id !== edge.id),
                  })
                }
              >
                {t('workflow.removeTransition')}
              </button>
            </fieldset>
          );
        })}
        <button
          type="button"
          disabled={
            draft.statuses.length < 2 || draft.transitions.length >= bounds.maximumTransitions
          }
          onClick={() =>
            onChange({
              ...draft,
              transitions: [
                ...draft.transitions,
                {
                  id: crypto.randomUUID(),
                  name: '',
                  from: draft.initial_status_id,
                  to: draft.statuses.find((status) => status.id !== draft.initial_status_id)!.id,
                  origins: ['manual'],
                  evidence: [],
                },
              ],
            })
          }
        >
          {t('workflow.addTransition')}
        </button>
      </fieldset>
    </>
  );
}
