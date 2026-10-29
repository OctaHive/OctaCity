import type { ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { aiSettingsPath } from '../../app/routes';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { catalogLabels } from './catalogLabels';
import { McpAuthorizationSummary } from './McpAuthorizationSummary';
import { ProfileCandidatePicker } from './ProfileCandidatePicker';
import type { FactoryRolesApi } from './roleApi';

type Schema = components['schemas'];
/** A selected Profile stays immutable; current installation metadata only explains its prerequisites. */
export function TriageStageProfile({
  title,
  projectId,
  api,
  value,
  onChange,
  children,
}: {
  title: string;
  projectId: string;
  api: FactoryRolesApi;
  value: Schema['FactoryRoleProfile'] | undefined;
  onChange: (value: Schema['FactoryRoleProfile']) => void;
  children?: ReactNode;
}) {
  const { t } = usePresentation();
  return (
    <section aria-label={title}>
      <h3>{title}</h3>
      {children}
      <ProfileCandidatePicker services={{ api, projectId }} onSelect={onChange} />
      <Link to={aiSettingsPath(projectId, 'runtimes')} target="_blank" rel="noreferrer">
        {t('factory.configureRuntime')}
      </Link>
      {value ? (
        <>
          <p>
            {value.definition.name} · v{value.reference.version}
          </p>
          <SelectedInstallation
            key={JSON.stringify(value.reference)}
            api={api}
            projectId={projectId}
            profile={value}
          />
        </>
      ) : null}
    </section>
  );
}
function SelectedInstallation({
  api,
  projectId,
  profile,
}: {
  api: FactoryRolesApi;
  projectId: string;
  profile: Schema['FactoryRoleProfile'];
}) {
  const { t } = usePresentation();
  const choices = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'capabilities'),
    async (cursor, signal) => {
      const page = await api.listCapabilities(projectId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
    },
  );
  const matches =
    choices.data?.pages
      .flatMap((page) => page.items)
      .filter(
        (item) =>
          item.build_configuration.id === profile.definition.build_configuration.id &&
          item.build_configuration.version === profile.definition.build_configuration.version &&
          item.build_configuration.project_id ===
            profile.definition.build_configuration.project_id &&
          item.build_configuration.definition_digest ===
            profile.definition.build_configuration.definition_digest &&
          (['plugin', 'executable', 'model'] as const).every(
            (key) =>
              sourceReferenceKey(item.toolchain[key]) ===
              sourceReferenceKey(profile.definition.toolchain[key]),
          ),
      ) ?? [];
  const alias = matches.length === 1 ? matches[0]!.alias : null;
  const details = useQuery({
    queryKey: queryKeys.factoryRoleCatalog(projectId, 'capability', alias ?? ''),
    queryFn: ({ signal }) => api.getCapability(projectId, alias!, signal),
    enabled: alias !== null,
  });
  return (
    <>
      <PagedExplorerBranch
        query={choices}
        empty={t('triage.installationMissing')}
        labels={catalogLabels(t('factory.choices'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {details.error ? (
        <QueryFailureNotice
          title={t('factory.choices')}
          error={details.error}
          onRetry={details.refetch}
        />
      ) : null}
      {Object.entries(profile.definition.execution?.mcp_servers ?? {}).map(([name, binding]) => {
        const installed = details.data?.mcp_configurations?.[name];
        const authorization =
          installed &&
          !details.isError &&
          sourceReferenceKey(installed.reference) === sourceReferenceKey(binding.configuration)
            ? installed.authorization
            : null;
        return (
          <section key={name}>
            <h4>{name}</h4>
            <p>
              {binding.configuration.identity} · {binding.configuration.version}
            </p>
            {authorization ? (
              <McpAuthorizationSummary authorization={authorization} />
            ) : (
              <p>{t('triage.mcpMissing')}</p>
            )}
          </section>
        );
      })}
      {!Object.keys(profile.definition.execution?.mcp_servers ?? {}).length ? (
        <p>{t('triage.mcpMissing')}</p>
      ) : null}
    </>
  );
}
