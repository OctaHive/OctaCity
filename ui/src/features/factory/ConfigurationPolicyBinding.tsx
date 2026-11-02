import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { factoryPolicyApi, type PolicyKind, type PolicyRef } from './policyApi';
import { PolicySummary } from './PolicySummary';
import { useFactoryPolicy } from './useFactoryPolicies';

type Document = components['schemas']['FactoryConfigurationDocument'];
type Binding = Document['admission_policy'] | Document['permission_ceiling'];

/** A legacy reference is looked up as saved, independent of the current defaults and catalog. */
export function useConfigurationPolicy(project: string, kind: PolicyKind, binding: Binding) {
  const legacy = 'identity' in binding ? { kind, reference: binding } : null;
  const mapping = useQuery({
    queryKey: queryKeys.factoryLegacyPolicy(project, legacy),
    queryFn: ({ signal }) =>
      legacy
        ? factoryPolicyApi.resolveLegacy(project, legacy, signal)
        : Promise.reject(new Error('Legacy policy reference is missing')),
    enabled: legacy !== null,
  });
  const mapped = mapping.data?.mapping?.target;
  const selected: PolicyRef | null =
    'project_policy' in binding
      ? { kind, reference: binding.project_policy }
      : mapped?.kind === kind
        ? mapped
        : null;
  return { legacy, mapping, selected };
}

export function ConfigurationPolicyOverview({
  project,
  kind,
  binding,
}: {
  project: string;
  kind: PolicyKind;
  binding: Binding;
}) {
  const { t } = usePresentation();
  const { legacy, mapping, selected } = useConfigurationPolicy(project, kind, binding);
  const exact = useFactoryPolicy(project, selected);
  const label = t(kind === 'launch' ? 'policy.launchLabel' : 'policy.accessLabel');
  const record = exact.data?.policy;
  const failure = mapping.error ?? exact.error;
  return (
    <section aria-label={label}>
      <h4>{label}</h4>
      {failure ? (
        <QueryFailureNotice
          title={t('policy.referenceUnavailable')}
          error={failure}
          onRetry={mapping.error ? mapping.refetch : exact.refetch}
        />
      ) : null}
      {(legacy && mapping.isPending) || (selected && exact.isPending) ? (
        <QueryLoadingNotice label={label} />
      ) : null}
      {legacy && mapping.isSuccess && !selected ? <p>{t('factory.unmappedLegacyPolicy')}</p> : null}
      {record ? (
        <>
          <p>
            {record.policy.definition.name} · v{record.policy.reference.version}
          </p>
          <PolicySummary record={record} />
        </>
      ) : null}
    </section>
  );
}
