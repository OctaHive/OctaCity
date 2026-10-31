import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { InstructionFields } from './InstructionFields';
type Node = components['schemas']['FactoryFlowNodeDefinition'];
/** The assignment belongs to this Flow version, independently of reusable Role/Profile instructions. */
export function StepTask({
  node,
  onChange,
}: {
  node: Node;
  onChange: (patch: Partial<Node>) => void;
}) {
  const { t } = usePresentation();
  if (node.kind !== 'reasoning') return null;
  return (
    <InstructionFields
      legend={t('factory.stepTask')}
      help={t('factory.stepTaskHelp')}
      label={t('factory.taskInstructions')}
      value={node.task ?? []}
      onChange={(task) => onChange({ task: task.length ? task : null })}
    />
  );
}
