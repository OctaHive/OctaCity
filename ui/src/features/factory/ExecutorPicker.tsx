import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { catalogLabels } from './catalogLabels';
import type { FactoryRolesApi } from './roleApi';

type Schema = components['schemas'];
type Props = {
  node: Schema['FactoryFlowNodeDefinition'];
  services: { api: FactoryRolesApi; projectId: string };
  requiredRole?: Schema['FactoryRoleReference'] | undefined;
  onRoleChange?: ((role: Schema['FactoryRoleReference'] | null) => void) | undefined;
  onResolved: (
    profile: Schema['FactoryRoleProfile'],
    build: Schema['FactoryFlowBuildBinding'],
  ) => void;
};

type Reference = Schema['FactoryRoleProfileReference'];
export function sameExecutorReference(a: Reference, b: Reference) {
  return (
    a.project_id === b.project_id &&
    a.id === b.id &&
    a.version === b.version &&
    a.digest === b.digest
  );
}
export function sameExecutorToolchain(
  a: Schema['FactoryRoleToolchain'],
  b: Schema['FactoryRoleToolchain'],
) {
  return (['plugin', 'executable', 'model'] as const).every(
    (key) =>
      a[key].identity === b[key].identity &&
      a[key].version === b[key].version &&
      a[key].digest === b[key].digest,
  );
}

export function ExecutorPicker({
  node,
  services: { api, projectId },
  requiredRole,
  onRoleChange,
  onResolved,
}: Props) {
  const { t } = usePresentation();
  const [roleId, setRoleId] = useState('');
  const [roleReference, setRoleReference] = useState<Reference>();
  const [profileReference, setProfileReference] = useState<Reference>();
  const [profileId, setProfileId] = useState('');
  const [runtime, setRuntime] = useState('');
  const [resultOutcome, setResultOutcome] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [problem, setProblem] = useState<'profile_incompatible' | 'build_unavailable' | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), []);
  const roles = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'roles'),
    async (cursor, signal) => {
      const page = await api.listRoles(projectId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.id })) };
    },
  );
  const profiles = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'profiles', roleId),
    async (cursor, signal) => {
      const page = await api.listProfiles(projectId, roleId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.id })) };
    },
    { enabled: !!roleId },
  );
  const capabilities = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'capabilities'),
    async (cursor, signal) => {
      const page = await api.listCapabilities(projectId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
    },
    { enabled: !!profileId },
  );
  const role = roles.data?.pages
    .flatMap((page) => page.items)
    .find(
      (item) =>
        item.id === roleId &&
        !!roleReference &&
        sameExecutorReference(item.reference, roleReference),
    );
  const availableProfiles =
    profiles.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => role && sameExecutorReference(item.role, role.reference)) ?? [];
  const profile = availableProfiles.find(
    (item) =>
      item.id === profileId &&
      !!profileReference &&
      sameExecutorReference(item.reference, profileReference),
  );
  const stale = (!!roleId && !role) || (!!profileId && !profile);
  const installed =
    capabilities.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => profile && sameExecutorToolchain(item.toolchain, profile.toolchain)) ?? [];
  const capability =
    installed.find((item) => item.alias === runtime) ??
    (installed.length === 1 ? installed[0] : undefined);
  const outcomes = node.outcomes.filter((item) => item.kind === 'success');
  const outcome =
    outcomes.find((item) => item.key === resultOutcome) ??
    (outcomes.length === 1 ? outcomes[0] : undefined);
  const cancel = () => {
    requestRef.current?.abort();
    setBusy(false);
    setError(null);
    setProblem(null);
  };
  async function apply() {
    if (!profile || !capability || !outcome || busy) return;
    cancel();
    const controller = new AbortController();
    requestRef.current = controller;
    setBusy(true);
    try {
      const exact = await api.getProfile(
        projectId,
        profile.reference.id,
        profile.reference.version,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      if (
        !sameExecutorReference(exact.reference, profile.reference) ||
        !role ||
        !sameExecutorReference(exact.role, role.reference)
      )
        throw new Error('Selected profile response does not match');
      const resolved = await api.resolveExecutionBinding(
        projectId,
        {
          profile: exact.reference,
          capability: capability.alias,
          result_outcome: outcome.key,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      if (!resolved.build || resolved.incompatibility) {
        setProblem(resolved.incompatibility ?? 'build_unavailable');
        return;
      }
      onResolved(exact, resolved.build);
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure instanceof Error ? failure : new Error());
    } finally {
      if (!controller.signal.aborted) setBusy(false);
    }
  }
  return (
    <>
      <PagedExplorerBranch
        query={roles}
        empty={t('factory.noAgents')}
        labels={catalogLabels(t('factory.executorRole'))}
      >
        {(items) => (
          <SelectMenu
            ariaLabel={t('factory.executorRole')}
            value={roleId}
            options={[
              { value: '', label: t('factory.chooseExecutorRole') },
              ...items
                .filter(
                  (item) => !requiredRole || sameExecutorReference(item.reference, requiredRole),
                )
                .map((item) => ({
                  value: item.id,
                  label: `${item.name} · v${item.reference.version}`,
                })),
            ]}
            onValueChange={(id) => {
              cancel();
              setRoleId(id);
              setRoleReference(items.find((item) => item.id === id)?.reference);
              setProfileReference(undefined);
              setProfileId('');
              setRuntime('');
              const chosen = items.find((item) => item.id === id);
              onRoleChange?.(chosen?.reference ?? null);
            }}
          />
        )}
      </PagedExplorerBranch>
      {roleId ? (
        <PagedExplorerBranch
          query={profiles}
          empty={t('factory.noProfiles')}
          labels={catalogLabels(t('factory.executorProfile'))}
        >
          {() =>
            availableProfiles.length ? (
              <SelectMenu
                ariaLabel={t('factory.executorProfile')}
                value={profileId}
                options={[
                  { value: '', label: t('factory.chooseExecutorProfile') },
                  ...availableProfiles.map((item) => ({
                    value: item.id,
                    label: `${item.name} · v${item.reference.version}`,
                  })),
                ]}
                onValueChange={(id) => {
                  cancel();
                  setProfileId(id);
                  setProfileReference(availableProfiles.find((item) => item.id === id)?.reference);
                  setRuntime('');
                }}
              />
            ) : (
              <p>{t('factory.noProfilesForRoleVersion')}</p>
            )
          }
        </PagedExplorerBranch>
      ) : null}
      {profileId ? (
        <PagedExplorerBranch
          query={capabilities}
          empty={t('factory.noInstalledExecutor')}
          labels={catalogLabels(t('factory.executorRuntime'))}
        >
          {() => (
            <>
              <SelectMenu
                ariaLabel={t('factory.executorRuntime')}
                value={capability?.alias ?? ''}
                options={[
                  { value: '', label: t('factory.chooseExecutorRuntime') },
                  ...installed.map((item) => ({
                    value: item.alias,
                    label: `${item.toolchain.executable.identity} · ${item.alias}`,
                  })),
                ]}
                onValueChange={(value) => {
                  cancel();
                  setRuntime(value);
                }}
              />
              {!installed.length && !stale ? (
                <p role="alert">{t('factory.noInstalledExecutor')}</p>
              ) : null}
            </>
          )}
        </PagedExplorerBranch>
      ) : null}
      {outcomes.length > 1 ? (
        <SelectMenu
          ariaLabel={t('factory.executorResultOutcome')}
          value={outcome?.key ?? ''}
          options={[
            { value: '', label: t('factory.chooseResultOutcome') },
            ...outcomes.map((item) => ({ value: item.key, label: item.key })),
          ]}
          onValueChange={(value) => {
            cancel();
            setResultOutcome(value);
          }}
        />
      ) : null}
      {stale ? <p role="alert">{t('factory.staleExecutorChoice')}</p> : null}
      {profileId && !outcomes.length ? (
        <p role="alert">{t('factory.executorNeedsOutcome')}</p>
      ) : null}
      <button
        type="button"
        disabled={
          !profile ||
          !capability ||
          !outcome ||
          busy ||
          roles.isError ||
          profiles.isError ||
          capabilities.isError
        }
        onClick={() => void apply()}
      >
        {t('factory.useExecutorProfile')}
      </button>
      {busy ? <p role="status">{t('factory.resolvingExecutor')}</p> : null}
      {problem ? (
        <p role="alert">
          {t(
            problem === 'profile_incompatible'
              ? 'factory.executorProfileIncompatible'
              : 'factory.noInstalledExecutor',
          )}
        </p>
      ) : null}
      {error ? (
        <QueryFailureNotice
          error={error}
          title={t('factory.executor')}
          onRetry={() => void apply()}
        />
      ) : null}
    </>
  );
}
