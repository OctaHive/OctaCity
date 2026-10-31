import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { ManagementValidationIssue } from '../../api/client';
import type { DraftIssue } from './validateDraft';
import { constraintSourceLabels } from './ConstraintReview';
const fields: Record<string, MessageKey> = {
  key: 'factory.nodeName',
  input_schema: 'factory.inputSchema',
  build: 'factory.executor',
  profile_selection: 'factory.executor',
  task: 'factory.stepTask',
  budget: 'factory.executionLimits',
  hard_budget: 'factory.executionLimits',
  permissions: 'factory.pipelineAccess',
  code: 'factory.stepCode',
  code_sources: 'factory.availableCode',
  context_binding: 'factory.documents',
  input_binding: 'factory.inputMapping',
  outcomes: 'factory.expectedResult',
  transitions: 'factory.controlTransitions',
  execution: 'factory.executionLimits',
  limits: 'factory.executionLimits',
  wip_limits: 'factory.startAndQueue',
};
export function validationFieldLabel(field: string, t: ReturnType<typeof usePresentation>['t']) {
  const node = /^\/root\/nodes\/(\d+)(?:\/|$)/u.exec(field);
  const parts = field.split('/').filter(Boolean);
  const key = (node ? parts.slice(3) : parts).find((key) => Object.hasOwn(fields, key));
  const label = t(key ? fields[key]! : 'factory.flowSettings');
  return node ? t('factory.stepFieldLabel', { number: Number(node[1]) + 1, field: label }) : label;
}
const resources = {
  attempts: 'factory.boundResource.attempts',
  elapsed_time: 'factory.boundResource.elapsed',
  tokens: 'factory.boundResource.tokens',
  cost: 'factory.boundResource.cost',
  output_bytes: 'factory.boundResource.output',
  active_nodes: 'factory.boundResource.active',
} satisfies Record<NonNullable<ManagementValidationIssue['resource']>, MessageKey>;
/** Closed public bound metadata only; private server messages and identities remain absent. */
export function LimitFailure({ issue }: { issue: DraftIssue | ManagementValidationIssue }) {
  const { t, locale } = usePresentation();
  const resource = 'resource' in issue ? issue.resource : undefined;
  const ceiling = 'ceiling' in issue ? issue.ceiling : undefined;
  const requested = 'requested' in issue ? issue.requested : undefined;
  const maximum = 'maximum' in issue ? issue.maximum : undefined;
  if (
    !resource ||
    !ceiling ||
    requested === undefined ||
    maximum === undefined ||
    !Number.isSafeInteger(requested) ||
    !Number.isSafeInteger(maximum)
  )
    return null;
  return (
    <p>
      {t('factory.boundFailure', {
        resource: t(resources[resource]),
        source: t(constraintSourceLabels[ceiling]),
        requested: requested.toLocaleString(locale),
        maximum: maximum.toLocaleString(locale),
      })}
    </p>
  );
}
