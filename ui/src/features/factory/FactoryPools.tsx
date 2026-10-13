import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { SelectMenu } from '../../shared/SelectMenu';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import type { FactoryApi, FactoryConfigurationDetails } from './api';

export function FactoryPools({
  api,
  configuration,
}: {
  api: Pick<FactoryApi, 'listPhaseReady'>;
  configuration: FactoryConfigurationDetails;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const options = Object.entries(configuration.pool_policy_digests ?? {}).map(([label, value]) => ({
    label,
    value,
  }));
  const selected =
    options.find((option) => option.value === params.get('policy_digest')) ?? options[0];
  if (selected === undefined) return null;
  return (
    <section aria-label={t('factory.pool')}>
      <SelectMenu
        ariaLabel={t('factory.pool')}
        options={options}
        value={selected.value}
        onValueChange={(digest) => {
          const next = new URLSearchParams(params);
          next.set('policy_digest', digest);
          next.delete('after');
          setParams(next);
        }}
      />
      <ReadyPage
        api={api}
        projectId={configuration.configuration.reference.project_id}
        digest={selected.value}
      />
    </section>
  );
}

function ReadyPage({
  api,
  projectId,
  digest,
}: {
  api: Pick<FactoryApi, 'listPhaseReady'>;
  projectId: string;
  digest: string;
}) {
  const { t } = usePresentation();
  const query = useCursorPage(queryKeys.factoryPool(projectId, digest), (cursor, signal) =>
    api.listPhaseReady(projectId, digest, cursor, signal),
  );
  return (
    <PagedExplorerBranch
      query={query}
      empty={t('factory.noReady')}
      labels={{
        failure: t('factory.pool'),
        loading: t('factory.pool'),
        stale: t('factory.pool'),
        loadMore: t('factory.moreReady'),
        loadingMore: t('factory.pool'),
      }}
    >
      {(items) => (
        <ul>
          {items.map((item) => (
            <li key={item.id}>
              <Link to={`/factory/${encodeURIComponent(item.run_id)}`}>{item.node}</Link>
              <span> · {item.severity}</span>
            </li>
          ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}
