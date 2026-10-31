import { useId, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import type { RoleServices } from './StageProfiles';
import { sameExecutorReference, sameExecutorToolchain } from './ExecutorPicker';
import { InstructionText } from './InstructionFields';
import { catalogLabels } from './catalogLabels';
import { McpAuthorizationSummary } from './McpAuthorizationSummary';
type S = components['schemas'];
function useAuthorizedProfile(reference: S['FactoryRoleProfileReference'], services: RoleServices) {
  const query = useQuery({
    queryKey: queryKeys.factoryRoleProfile(reference),
    queryFn: ({ signal }) =>
      services.api.getProfile(reference.project_id, reference.id, reference.version, signal),
  });
  return {
    ...query,
    exact:
      !query.error && query.data && sameExecutorReference(query.data.reference, reference)
        ? query.data
        : undefined,
  };
}
/** Read-only exact metadata. A retained draft selection never substitutes for current authorization. */
export function ExecutorSummary({
  reference,
  services,
}: {
  reference: S['FactoryRoleProfileReference'];
  services: RoleServices;
}) {
  const { t } = usePresentation();
  const query = useAuthorizedProfile(reference, services);
  const [view, setView] = useState(false);
  const id = useId();
  const linkRef = useRef<HTMLAnchorElement>(null);
  if (!query.exact)
    return query.isPending ? (
      <QueryLoadingNotice label={t('factory.executorProfile')} />
    ) : (
      <QueryFailureNotice
        title={t('factory.profileMetadataUnavailable')}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
    );
  const profile = query.exact;
  const definition = profile.definition;
  return (
    <section>
      <p>{definition.model_identifier ?? definition.toolchain.model.identity}</p>
      <p>
        {definition.toolchain.executable.identity} · {definition.toolchain.executable.version}
      </p>
      <p>
        {t('factory.profileSkills', {
          skills: Object.keys(definition.skills).join(', ') || t('factory.none'),
        })}
      </p>
      <a
        ref={linkRef}
        href={`#${id}`}
        onClick={(event) => {
          event.preventDefault();
          setView(true);
        }}
      >
        {t('factory.viewExactProfile', { name: definition.name, version: reference.version })}
      </a>
      {view ? (
        <section
          id={id}
          role="region"
          aria-label={t('factory.exactProfileTitle', {
            name: definition.name,
            version: reference.version,
          })}
          tabIndex={-1}
          ref={(element) => {
            element?.focus();
          }}
        >
          <h4>
            {t('factory.exactProfileTitle', { name: definition.name, version: reference.version })}
          </h4>
          <InstructionText value={definition.instructions} />
          <button
            type="button"
            onClick={() => {
              setView(false);
              linkRef.current?.focus();
            }}
          >
            {t('factory.backToStep')}
          </button>
        </section>
      ) : null}
      <details>
        <summary>{t('factory.routeDetails')}</summary>
        <p>
          {reference.id} · {reference.digest}
        </p>
        <p>
          {definition.build_configuration.id} · v{definition.build_configuration.version}
        </p>
      </details>
    </section>
  );
}
export function StepTools({
  references,
  services,
}: {
  references: S['FactoryRoleProfileReference'][];
  services: RoleServices;
}) {
  const { t } = usePresentation();
  return (
    <fieldset>
      <legend>{t('factory.toolsAndAccess')}</legend>
      <p>{t('factory.configuredAccessOnly')}</p>
      {references.map((reference) => (
        <ProfileTools
          key={`${reference.id}:${reference.version}:${reference.digest}`}
          reference={reference}
          services={services}
        />
      ))}
      {!references.length ? <p>{t('factory.noProfileTools')}</p> : null}
    </fieldset>
  );
}
function ProfileTools({
  reference,
  services,
}: {
  reference: S['FactoryRoleProfileReference'];
  services: RoleServices;
}) {
  const { t } = usePresentation();
  const profile = useAuthorizedProfile(reference, services);
  const choices = useCursorPage(
    queryKeys.factoryRoleCatalog(reference.project_id, 'capabilities'),
    async (cursor, signal) => {
      const page = await services.api.listCapabilities(reference.project_id, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
    },
  );
  const choice = choices.data?.pages
    .flatMap((page) => page.items)
    .find(
      (item) =>
        profile.exact && sameExecutorToolchain(item.toolchain, profile.exact.definition.toolchain),
    );
  const capability = useQuery({
    queryKey: queryKeys.factoryRoleCatalog(reference.project_id, 'capability', choice?.alias ?? ''),
    enabled: !!choice && !!profile.exact,
    queryFn: ({ signal }) =>
      services.api.getCapability(reference.project_id, choice!.alias, signal),
  });
  if (!profile.exact)
    return profile.isPending ? (
      <QueryLoadingNotice label={t('factory.executorProfile')} />
    ) : (
      <p role="alert">{t('factory.profileMetadataUnavailable')}</p>
    );
  const definition = profile.exact.definition;
  return (
    <section>
      <h5>
        {definition.name} · v{reference.version}
      </h5>
      <p>
        {t('factory.permissionCounts', {
          plugins: definition.permissions.plugins.length,
          tools: definition.permissions.tools.length,
          hosts: definition.permissions.network_hosts.length,
        })}
      </p>
      <PagedExplorerBranch
        query={choices}
        labels={catalogLabels(t('factory.executorRuntime'))}
        empty={t('factory.noInstalledExecutor')}
      >
        {() => null}
      </PagedExplorerBranch>
      {Object.entries(definition.execution?.mcp_servers ?? {}).map(([alias, binding]) => {
        const details = capability.data?.mcp_configurations?.[alias];
        const exact =
          details &&
          details.reference.identity === binding.configuration.identity &&
          details.reference.version === binding.configuration.version &&
          details.reference.digest === binding.configuration.digest &&
          !capability.error
            ? details
            : undefined;
        return (
          <section key={alias}>
            <h5>MCP · {alias}</h5>
            {exact?.authorization ? (
              <p>{t('runtime.readOnlyAccess')}</p>
            ) : choices.isPending || (choice && capability.isPending) ? (
              <QueryLoadingNotice label={t('factory.mcp')} />
            ) : (
              <p role="alert">{t('factory.mcpMetadataUnavailable')}</p>
            )}
            {exact ? (
              <p>
                {t('factory.mcpToolScope', {
                  tools:
                    exact.server.enabled_tools?.join(', ') ?? t('factory.mcpServerDefinedTools'),
                })}
              </p>
            ) : null}
            <details>
              <summary>{t('factory.routeDetails')}</summary>
              <p>
                {binding.configuration.identity} · {binding.configuration.version} ·{' '}
                {binding.configuration.digest}
              </p>
              {exact?.authorization ? (
                <McpAuthorizationSummary authorization={exact.authorization} />
              ) : null}
            </details>
          </section>
        );
      })}
      {!Object.keys(definition.execution?.mcp_servers ?? {}).length ? (
        <p>{t('factory.noRuntimeMcp')}</p>
      ) : null}
      <details>
        <summary>{t('factory.routeDetails')}</summary>
        <ul>
          {definition.permissions.secret_profiles.map((name) => (
            <li key={name}>{name}</li>
          ))}
          {Object.entries(definition.execution?.integrations ?? {}).map(([name, binding]) => (
            <li key={name}>
              {name} · {binding.connection_id} · v{binding.connection_version} ·{' '}
              {binding.resource.identity}
            </li>
          ))}
        </ul>
      </details>
    </section>
  );
}
