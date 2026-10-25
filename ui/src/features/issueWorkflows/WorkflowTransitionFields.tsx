import { useEffect, useRef, useState } from 'react';
import { ISSUE_WORKFLOW_CONSTRAINTS as bounds } from '../../../.generated/api/issue-workflow';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import type { WorkflowDefinition } from '../../api/issueWorkflows';
import styles from './WorkflowView.module.css';

/** Existing transition policy fields for the single arrow selected on the canvas. */
export function WorkflowTransitionFields({
  draft,
  edge,
  onChange,
  onRemove,
}: {
  draft: WorkflowDefinition;
  onRemove?: ((id: string) => void) | undefined;
  edge: WorkflowDefinition['transitions'][number];
  onChange: (draft: WorkflowDefinition) => void;
}) {
  const { t } = usePresentation();
  const index = draft.transitions.findIndex((item) => item.id === edge.id);
  const nameRef = useRef<HTMLInputElement>(null);
  useEffect(() => nameRef.current?.focus(), [edge.id]);
  const update = (patch: Partial<typeof edge>) =>
    onChange({
      ...draft,
      transitions: draft.transitions.map((item) =>
        item.id === edge.id ? { ...item, ...patch } : item,
      ),
    });
  return (
    <fieldset>
      <legend>{t('workflow.transitionNumber', { number: index + 1 })}</legend>
      <label>
        {t('workflow.transitionName')}
        <input
          ref={nameRef}
          value={edge.name}
          maxLength={bounds.nameMaximumBytes}
          onChange={(event) => update({ name: event.target.value })}
        />
      </label>
      <WorkflowEndpoints draft={draft} value={{ from: edge.from, to: edge.to }} onChange={update} />
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
      {onRemove ? (
        <button type="button" onClick={() => onRemove(edge.id)}>
          {t('workflow.removeTransition')}
        </button>
      ) : null}
    </fieldset>
  );
}

type Endpoints = Pick<WorkflowDefinition['transitions'][number], 'from' | 'to'>;

function WorkflowEndpoints({
  draft,
  value,
  onChange,
}: {
  draft: WorkflowDefinition;
  value: Endpoints;
  onChange: (value: Endpoints) => void;
}) {
  const { t } = usePresentation();
  const options = draft.statuses.map((item) => ({
    value: item.id,
    label: item.name || t('workflow.unnamedStatus'),
  }));
  return (
    <div className={styles.endpoints}>
      {(['from', 'to'] as const).map((endpoint) => (
        <div key={endpoint}>
          {t(`workflow.${endpoint}`)}
          <SelectMenu
            ariaLabel={t(`workflow.${endpoint}`)}
            options={options}
            value={value[endpoint]}
            onValueChange={(id) => onChange({ ...value, [endpoint]: id })}
          />
        </div>
      ))}
    </div>
  );
}

export type WorkflowConnection = {
  from: string;
  onConnect: (value: Endpoints) => void;
  onCancel: () => void;
};

/** Keyboard equivalent of connecting a source handle to a destination card. */
export function WorkflowConnectionFields({
  draft,
  connection,
}: {
  draft: WorkflowDefinition;
  connection: WorkflowConnection;
}) {
  const { t } = usePresentation();
  const formRef = useRef<HTMLFieldSetElement>(null);
  useEffect(() => {
    formRef.current?.querySelector<HTMLButtonElement>('[role="combobox"]')?.focus();
  }, []);
  const [value, setValue] = useState<Endpoints>(() => ({
    from: connection.from,
    to: draft.statuses.find((status) => status.id !== connection.from)?.id ?? '',
  }));
  return (
    <fieldset ref={formRef}>
      <legend>{t('workflow.addTransition')}</legend>
      <WorkflowEndpoints draft={draft} value={value} onChange={setValue} />
      <button
        type="button"
        disabled={value.from === value.to || !value.from || !value.to}
        onClick={() => connection.onConnect(value)}
      >
        {t('workflow.connect')}
      </button>
      <button type="button" onClick={connection.onCancel}>
        {t('workflow.cancelConnection')}
      </button>
    </fieldset>
  );
}
