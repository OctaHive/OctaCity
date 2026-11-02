import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { FactoryPolicyApi, PolicyRecord } from './policyApi';

/** Changing defaults is a separate reviewed operation; sibling defaults and saved pins are retained. */
export function PolicyDefaultSelection({
  project,
  api,
  record,
}: {
  project: string;
  api: FactoryPolicyApi;
  record: PolicyRecord;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const defaults = useQuery({
    queryKey: queryKeys.factoryPolicyDefaults(project),
    queryFn: ({ signal }) => api.defaults(project, signal),
  });
  const [review, setReview] = useState<{
    request: components['schemas']['FactoryProjectPolicyDefaultsDraft'];
    target: HTMLElement;
  } | null>(null);
  return (
    <>
      <button
        type="button"
        disabled={!defaults.data || !!defaults.error || defaults.isFetching}
        onClick={(event) => {
          const current = defaults.data?.defaults;
          setReview({
            target: event.currentTarget,
            request: {
              expected_version: current?.version ?? null,
              launch_policy:
                record.kind === 'launch'
                  ? record.policy.reference
                  : (current?.launch_policy ?? null),
              access_profile:
                record.kind === 'access'
                  ? record.policy.reference
                  : (current?.access_profile ?? null),
            },
          });
        }}
      >
        {t('policy.useDefault')}
      </button>
      {review ? (
        <ConfirmedCommand
          title={t('policy.defaults')}
          confirmLabel={t('policy.confirmDefault')}
          consequence={t('policy.defaultConsequence', {
            name: record.policy.definition.name,
            version: record.policy.reference.version,
          })}
          request={review.request}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryPolicyDefaults(project) },
            { queryKey: queryKeys.factoryChoices(project) },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const result = await api.selectDefaults(
              project,
              structuredClone(
                attempt.request,
              ) as components['schemas']['FactoryProjectPolicyDefaultsDraft'],
              attempt.headers,
            );
            client.setQueryData(queryKeys.factoryPolicyDefaults(project), {
              defaults: result.defaults,
            });
            return result;
          }}
          onRefresh={async () => {
            const current = await api.defaults(project);
            client.setQueryData(queryKeys.factoryPolicyDefaults(project), current);
          }}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('policy.defaultSaved')}</p>}
        />
      ) : null}
    </>
  );
}
