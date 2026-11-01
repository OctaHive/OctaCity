import { useState } from 'react';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
/** Selects the authoritative admitted task without adding it to model result data. */
export function FlowWorkTask({ node, onChange }: { node: Node; onChange: (node: Node) => void }) {
  const { t } = usePresentation();
  const [name, setName] = useState(node.context_binding?.work_task ?? 'task');
  const binding = node.context_binding;
  const occupied = !!binding?.entries[name] || !!binding?.artifact_collections?.[name];
  const bounds = FACTORY_AUTHORING_SCHEMAS.FactoryFlowContextBinding.properties.work_task;
  const valid = name.length <= bounds.maxLength && new RegExp(bounds.pattern, 'u').test(name);
  return (
    <fieldset data-factory-field="/context_binding/work_task" tabIndex={-1}>
      <legend>{t('factory.context.task')}</legend>
      <label>
        <input
          type="checkbox"
          checked={!!binding?.work_task}
          disabled={occupied || !valid}
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
      {occupied ? <p role="alert">{t('factory.workTaskOccupied')}</p> : null}
      <details>
        <summary>{t('factory.workTaskDetails')}</summary>
        <label>
          {t('factory.workTaskName')}
          <input
            value={name}
            maxLength={bounds.maxLength}
            disabled={!!binding?.work_task}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
      </details>
    </fieldset>
  );
}
