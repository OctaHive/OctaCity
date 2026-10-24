import { useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { catalogLabels } from './catalogLabels';
import type { RoleServices } from './StageProfiles';
import type { components } from '../../../.generated/api/schema';
/** Reads authorized exact versions, without evaluating or qualifying a candidate locally. */
export function ProfileCandidatePicker({
  services: { api, projectId },
  onSelect,
}: {
  services: RoleServices;
  onSelect: (profile: components['schemas']['FactoryRoleProfile']) => void;
}) {
  const { t } = usePresentation();
  const [role, setRole] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const roles = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'roles'),
    async (cursor, signal) => {
      const page = await api.listRoles(projectId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.id })) };
    },
  );
  return (
    <section>
      <PagedExplorerBranch
        query={roles}
        empty={t('factory.noAgents')}
        labels={catalogLabels(t('factory.agents'))}
      >
        {(items) => (
          <ul>
            {items.map((item) => (
              <li key={item.id}>
                <button type="button" onClick={() => setRole(item.id)}>
                  {item.name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {role ? (
        <CandidateProfiles
          key={role}
          project={projectId}
          api={api}
          role={role}
          disabled={busy}
          onSelect={async (reference) => {
            setBusy(true);
            setError(null);
            try {
              onSelect(await api.getProfile(projectId, reference.id, reference.version));
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            } finally {
              setBusy(false);
            }
          }}
        />
      ) : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.profiles')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
    </section>
  );
}
function CandidateProfiles({
  api,
  project,
  role,
  onSelect,
  disabled,
}: {
  api: RoleServices['api'];
  project: string;
  role: string;
  disabled: boolean;
  onSelect: (reference: components['schemas']['FactoryRoleProfileReference']) => void;
}) {
  const { t } = usePresentation();
  const profiles = useCursorPage(
    queryKeys.factoryRoleCatalog(project, 'profiles', role),
    async (cursor, signal) => {
      const page = await api.listProfiles(project, role, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.id })) };
    },
  );
  return (
    <PagedExplorerBranch
      query={profiles}
      empty={t('factory.noProfiles')}
      labels={catalogLabels(t('factory.profiles'))}
    >
      {(items) => (
        <ul>
          {items.map((item) => (
            <li key={item.id}>
              <button type="button" disabled={disabled} onClick={() => onSelect(item.reference)}>
                {item.name} · v{item.reference.version}
              </button>
            </li>
          ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}
