import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { PolicyEditor } from './PolicyEditor';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import {
  factoryPolicyApi,
  type FactoryPolicyApi,
  type PolicyRecord,
  type PolicyRef,
} from './policyApi';
import { PolicyDefaultSelection } from './PolicyDefaultSelection';
import { PolicySummary } from './PolicySummary';
import { useFactoryPolicy, useFactoryPolicyCatalog } from './useFactoryPolicies';
import styles from './Factory.module.css';

/** Catalog selection and read-only inspection remain separate from confirmed publication. */
export function FactoryPolicies({
  project,
  api = factoryPolicyApi,
  initialSelection = null,
  onPublished,
}: {
  project: string;
  api?: FactoryPolicyApi;
  initialSelection?: PolicyRef | null;
  onPublished?: (record: PolicyRecord) => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [draft, setDraft] = useState<components['schemas']['FactoryProjectPolicyDraft'] | null>(
    null,
  );
  const [selected, setSelected] = useState(initialSelection);
  const catalog = useFactoryPolicyCatalog(project, api);
  const defaults = useQuery({
    queryKey: queryKeys.factoryPolicyDefaults(project),
    queryFn: ({ signal }) => api.defaults(project, signal),
  });
  return (
    <section className={styles.workspace} aria-label={t('policy.settings')}>
      <h2>{t('policy.settings')}</h2>
      <p>{t('policy.help')}</p>
      {catalog.isPending ? <QueryLoadingNotice label={t('policy.settings')} /> : null}
      {catalog.error ? (
        <QueryFailureNotice
          title={t('policy.settings')}
          error={catalog.error}
          onRetry={catalog.refetch}
        />
      ) : null}
      {defaults.error ? (
        <QueryFailureNotice
          title={t('policy.defaults')}
          error={defaults.error}
          onRetry={defaults.refetch}
        />
      ) : null}
      {(['launch', 'access'] as const).map((kind) => {
        const pin =
          kind === 'launch'
            ? defaults.data?.defaults?.launch_policy
            : defaults.data?.defaults?.access_profile;
        const items =
          catalog.data?.pages
            .flatMap((page) => page.items)
            .filter((item) => item.reference.kind === kind) ?? [];
        return (
          <section
            key={kind}
            aria-label={t(kind === 'launch' ? 'policy.launches' : 'policy.accesses')}
          >
            <h3>{t(kind === 'launch' ? 'policy.launches' : 'policy.accesses')}</h3>
            {pin ? (
              <button type="button" onClick={() => setSelected({ kind, reference: pin })}>
                {t('policy.defaultVersion', { version: pin.version })}
              </button>
            ) : defaults.data ? (
              <p>{t('policy.noDefault')}</p>
            ) : null}
            {!catalog.isPending && !catalog.error && !items.length ? (
              <p>{t('policy.empty')}</p>
            ) : null}
            <button
              type="button"
              disabled={draft !== null}
              onClick={() =>
                setDraft(
                  kind === 'launch'
                    ? {
                        kind,
                        id: crypto.randomUUID(),
                        expected_current: null,
                        definition: {
                          name: '',
                          allow_manual: false,
                          allow_automatic_source: false,
                        },
                      }
                    : {
                        kind,
                        id: crypto.randomUUID(),
                        expected_current: null,
                        definition: { name: '', permissions: emptyFactoryPermissions() },
                      },
                )
              }
            >
              {t(kind === 'launch' ? 'policy.newLaunch' : 'policy.newAccess')}
            </button>
            <ul>
              {items.map((item) => (
                <li key={item.reference.reference.id}>
                  <button type="button" onClick={() => setSelected(item.reference)}>
                    {t('policy.view', {
                      name: item.name,
                      version: item.reference.reference.version,
                    })}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
      {catalog.hasNextPage ? (
        <button
          type="button"
          disabled={catalog.isFetchingNextPage}
          onClick={() => void catalog.fetchNextPage()}
        >
          {t('policy.more')}
        </button>
      ) : null}
      {draft ? (
        <PolicyEditor
          project={project}
          api={api}
          initial={draft}
          onCancel={() => setDraft(null)}
          onPublished={(record) => {
            const reference = { kind: record.kind, reference: record.policy.reference };
            client.setQueryData(queryKeys.factoryPolicy(project, reference), { policy: record });
            setSelected(reference);
            setDraft(null);
            onPublished?.(record);
          }}
        />
      ) : null}
      {!draft && selected ? (
        <PolicyDetails
          key={JSON.stringify(selected)}
          project={project}
          selected={selected}
          api={api}
          onEdit={(record) => setDraft(replacementDraft(record))}
        />
      ) : null}
    </section>
  );
}
function PolicyDetails({
  project,
  selected,
  api,
  onEdit,
}: {
  project: string;
  selected: PolicyRef;
  api: FactoryPolicyApi;
  onEdit: (record: PolicyRecord) => void;
}) {
  const { t } = usePresentation();
  const query = useFactoryPolicy(project, selected, api);
  if (query.isPending) return <QueryLoadingNotice label={t('policy.details')} />;
  if (!query.data)
    return (
      <QueryFailureNotice title={t('policy.details')} error={query.error} onRetry={query.refetch} />
    );
  return (
    <section aria-label={t('policy.details')}>
      <h3>
        {query.data.policy.policy.definition.name} · v{query.data.policy.policy.reference.version}
      </h3>
      <PolicySummary record={query.data.policy} />
      <PolicyDefaultSelection project={project} api={api} record={query.data.policy} />
      <button type="button" onClick={() => onEdit(query.data.policy)}>
        {t('policy.edit')}
      </button>
      <details>
        <summary>{t('factory.routeDetails')}</summary>
        <dl>
          <dt>{t('factory.digest')}</dt>
          <dd>{query.data.policy.policy.reference.digest}</dd>
        </dl>
      </details>
    </section>
  );
}

function replacementDraft(
  record: PolicyRecord,
): components['schemas']['FactoryProjectPolicyDraft'] {
  const reference = record.policy.reference;
  const base = { id: reference.id, expected_current: reference };
  return record.kind === 'launch'
    ? { ...base, kind: 'launch', definition: record.policy.definition }
    : { ...base, kind: 'access', definition: record.policy.definition };
}
