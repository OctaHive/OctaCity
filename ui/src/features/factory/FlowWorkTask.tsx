import { useState } from 'react';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
/** Selects owner-frozen Work documents without changing the step's typed input or result. */
export function FlowWorkTask({ node, onChange }: { node: Node; onChange: (node: Node) => void }) {
  const { t } = usePresentation();
  const [name, setName] = useState(node.context_binding?.work_task ?? 'task');
  const [criteriaName, setCriteriaName] = useState(
    node.context_binding?.work_acceptance ?? 'work_acceptance',
  );
  const binding = node.context_binding;
  const bounds = FACTORY_AUTHORING_SCHEMAS.FactoryFlowContextBinding.properties;
  const occupied = (key: string) =>
    !!binding?.entries[key] || !!binding?.artifact_collections?.[key];
  const valid = (key: string) =>
    key.length <= bounds.work_task.maxLength && new RegExp(bounds.work_task.pattern, 'u').test(key);
  const criteriaOccupied = occupied(criteriaName) || binding?.work_task === criteriaName;
  const contextFull =
    Object.keys(binding?.entries ?? {}).length +
      Object.keys(binding?.artifact_collections ?? {}).length +
      Number(!!binding?.work_task) >=
    bounds.entries.maxProperties;
  return (
    <fieldset data-factory-field="/context_binding/work_task" tabIndex={-1}>
      <legend>{t('factory.context.task')}</legend>
      <label>
        <input
          type="checkbox"
          checked={!!binding?.work_task}
          disabled={!!binding?.work_acceptance || occupied(name) || !valid(name)}
          onChange={(event) => {
            const next = { ...binding, entries: binding?.entries ?? {} };
            if (event.target.checked) next.work_task = name;
            else delete next.work_task;
            const retained =
              next.work_task ||
              Object.keys(next.entries).length ||
              Object.keys(next.artifact_collections ?? {}).length ||
              next.candidate_revision;
            onChange({ ...node, context_binding: retained ? next : null });
          }}
        />
        {t('factory.workTask')}
      </label>
      <p>{t('factory.workTaskHelp')}</p>
      {occupied(name) ? <p role="alert">{t('factory.workTaskOccupied')}</p> : null}
      <label>
        <input
          type="checkbox"
          checked={!!binding?.work_acceptance}
          disabled={
            !binding?.work_acceptance &&
            (!binding?.work_task || criteriaOccupied || !valid(criteriaName) || contextFull)
          }
          onChange={(event) => {
            if (!binding?.work_task) return;
            const next = { ...binding };
            if (event.target.checked) next.work_acceptance = criteriaName;
            else delete next.work_acceptance;
            onChange({ ...node, context_binding: next });
          }}
        />
        {t('factory.workAcceptance')}
      </label>
      <p>{t('factory.workAcceptanceHelp')}</p>
      {binding?.work_task && criteriaOccupied ? (
        <p role="alert">{t('factory.workAcceptanceOccupied')}</p>
      ) : null}
      {binding?.work_task && contextFull && !binding.work_acceptance ? (
        <p role="alert">{t('factory.workAcceptanceContextFull')}</p>
      ) : null}
      <details>
        <summary>{t('factory.workTaskDetails')}</summary>
        <label>
          {t('factory.workTaskName')}
          <input
            value={name}
            maxLength={bounds.work_task.maxLength}
            disabled={!!binding?.work_task}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label>
          {t('factory.workAcceptanceName')}
          <input
            value={criteriaName}
            maxLength={bounds.work_task.maxLength}
            disabled={!!binding?.work_acceptance}
            onChange={(event) => setCriteriaName(event.target.value)}
          />
        </label>
      </details>
    </fieldset>
  );
}
