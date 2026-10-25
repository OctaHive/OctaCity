import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { WorkflowDefinition } from '../../api/issueWorkflows';

/** One human-readable policy review shared by publication and exact default selection. */
export function useWorkflowSummary(definition: WorkflowDefinition): string {
  const { t } = usePresentation();
  const statusName = (id: string) =>
    definition.statuses.find((status) => status.id === id)?.name ?? id;
  const parts = [
    `${t('workflow.statuses')}: ${definition.statuses.map((status) => status.name).join(', ')}`,
    `${t('workflow.initial')}: ${statusName(definition.initial_status_id)}`,
    `${t('workflow.terminals')}: ${definition.terminal_status_ids.map(statusName).join(', ') || t('workflow.none')}`,
  ];
  for (const edge of definition.transitions) {
    const origins = edge.origins.map((origin) => t(`workflow.origin.${origin}`)).join(', ');
    const evidence =
      edge.evidence.map((item) => t(`workflow.evidence.${item}`)).join(', ') || t('workflow.none');
    parts.push(
      `${edge.name}: ${statusName(edge.from)} → ${statusName(edge.to)}; ${t('workflow.origins')}: ${origins}; ${t('workflow.evidence')}: ${evidence}`,
    );
  }
  return parts.join('. ');
}
