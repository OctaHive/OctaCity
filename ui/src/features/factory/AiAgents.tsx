import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import type { FactoryRolesApi } from './roleApi';
import { AgentIcon } from './AgentFields';
import { TemplateCopy } from './TemplateCopy';
import { ProfileEditor } from './ProfileEditor';
import { InstructionText } from './InstructionFields';
import { catalogLabels as labels } from './catalogLabels';
import { AgentEditor } from './AgentEditor';
import { SkillReferences } from './SkillContents';

type Reference = components['schemas']['FactoryRoleReference'];
export function AiAgents({ api, projectId }: { api: FactoryRolesApi; projectId: string }) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<Reference | null>(null);
  const [template, setTemplate] = useState<
    components['schemas']['FactoryImmutableReferenceResource'] | null
  >(null);
  const templates = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'templates'),
    async (cursor, signal) => {
      const page = await api.listTemplates(projectId, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.digest })) };
    },
  );
  const [creating, setCreating] = useState(false);
  const roles = useCursorPage(
    queryKeys.factoryRoleCatalog(projectId, 'roles'),
    async (cursor, signal) => {
      const page = await api.listRoles(projectId, cursor, signal);
      return { ...page, items: page.items.map((role) => ({ ...role, id: role.reference.id })) };
    },
  );
  return (
    <section aria-label={t('factory.agents')}>
      <h2>{t('factory.agents')}</h2>
      <button type="button" disabled={creating} onClick={() => setCreating(true)}>
        {t('factory.createAgent')}
      </button>
      {creating ? (
        <AgentEditor api={api} projectId={projectId} onDiscard={() => setCreating(false)} />
      ) : null}
      <PagedExplorerBranch
        query={templates}
        labels={labels(t('factory.templates'))}
        empty={t('factory.noTemplates')}
      >
        {(items) => (
          <ul>
            {items.map((item) => (
              <li key={item.id}>
                <button type="button" onClick={() => setTemplate(item.reference)}>
                  {t('factory.copyNamedTemplate', { name: item.name })}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {template ? (
        <TemplateCopy
          key={template.digest}
          reference={template}
          project={projectId}
          api={api}
          onDiscard={() => setTemplate(null)}
        />
      ) : null}
      <PagedExplorerBranch
        query={roles}
        labels={labels(t('factory.agents'))}
        empty={t('factory.noAgents')}
      >
        {(items) => (
          <ul>
            {items.map((role) => (
              <li key={role.id}>
                <button type="button" onClick={() => setSelected(role.reference)}>
                  {role.name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {selected ? (
        <AgentDetails key={`${selected.id}:${selected.version}`} api={api} reference={selected} />
      ) : null}
    </section>
  );
}
function AgentDetails({ api, reference }: { api: FactoryRolesApi; reference: Reference }) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<Reference | null>(null);
  const [editing, setEditing] = useState(false);
  const [creating, setCreating] = useState(false);
  const role = useQuery({
    queryKey: queryKeys.factoryRole(reference),
    queryFn: ({ signal }) =>
      api.getRole(reference.project_id, reference.id, reference.version, signal),
  });
  const profiles = useCursorPage(
    queryKeys.factoryRoleCatalog(reference.project_id, 'profiles', reference.id),
    async (cursor, signal) => {
      const page = await api.listProfiles(reference.project_id, reference.id, cursor, signal);
      return {
        ...page,
        items: page.items.map((profile) => ({ ...profile, id: profile.reference.id })),
      };
    },
  );
  if (!role.data)
    return role.isPending ? (
      <QueryLoadingNotice label={t('factory.agents')} />
    ) : (
      <QueryFailureNotice title={t('factory.agents')} error={role.error} onRetry={role.refetch} />
    );
  return (
    <section>
      <button type="button" onClick={() => setEditing(true)}>
        {t('factory.editAgent')}
      </button>
      <button type="button" onClick={() => setCreating(true)}>
        {t('factory.createProfile')}
      </button>
      {editing ? (
        <AgentEditor
          api={api}
          projectId={reference.project_id}
          baseline={role.data}
          onDiscard={() => setEditing(false)}
        />
      ) : null}
      {creating ? (
        <ProfileEditor api={api} role={role.data} onDiscard={() => setCreating(false)} />
      ) : null}
      <h3>
        <AgentIcon name={role.data.definition.icon} avatar={role.data.definition.avatar} />{' '}
        {role.data.definition.name} · v{reference.version}
      </h3>
      <p>{role.data.definition.description}</p>
      <InstructionText value={role.data.definition.instructions} />
      <SkillReferences
        api={api}
        project={reference.project_id}
        skills={role.data.definition.skills ?? {}}
      />
      <PagedExplorerBranch
        query={profiles}
        labels={labels(t('factory.profiles'))}
        empty={t('factory.noProfiles')}
      >
        {(items) => (
          <ul>
            {items.map((profile) => (
              <li key={profile.id}>
                <button type="button" onClick={() => setSelected(profile.reference)}>
                  {profile.name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {selected ? (
        <ProfileDetails
          key={`${selected.id}:${selected.version}`}
          api={api}
          reference={selected}
          currentRole={role.data}
        />
      ) : null}
    </section>
  );
}
function ProfileDetails({
  api,
  reference,
  currentRole,
}: {
  api: FactoryRolesApi;
  reference: Reference;
  currentRole: components['schemas']['FactoryRole'];
}) {
  const { t } = usePresentation();
  const [editing, setEditing] = useState(false);
  const profile = useQuery({
    queryKey: queryKeys.factoryRoleProfile(reference),
    queryFn: ({ signal }) =>
      api.getProfile(reference.project_id, reference.id, reference.version, signal),
  });
  if (!profile.data)
    return profile.isPending ? (
      <QueryLoadingNotice label={t('factory.profiles')} />
    ) : (
      <QueryFailureNotice
        title={t('factory.profiles')}
        error={profile.error}
        onRetry={profile.refetch}
      />
    );
  const definition = profile.data.definition;
  return (
    <section>
      <button type="button" onClick={() => setEditing(true)}>
        {t('factory.editProfile')}
      </button>
      {editing ? (
        <ProfileEditor
          api={api}
          role={currentRole}
          baseline={profile.data}
          onDiscard={() => setEditing(false)}
        />
      ) : null}
      <h4>
        {definition.name} · v{reference.version}
      </h4>
      <PinnedRole api={api} reference={profile.data.role} />
      <InstructionText value={definition.instructions} />
      <ul>
        {Object.entries(definition.skills).map(([alias, ref]) => (
          <li key={alias}>
            {alias} · {ref.identity} · {ref.version} · {ref.digest}
          </li>
        ))}
      </ul>
      <p>
        {definition.toolchain.plugin.identity} · {definition.toolchain.model.identity}
      </p>
      <p>
        {definition.thinking_parameter
          ? String(definition.model_parameters[definition.thinking_parameter])
          : t('factory.none')}
      </p>
    </section>
  );
}

function PinnedRole({ api, reference }: { api: FactoryRolesApi; reference: Reference }) {
  const { t } = usePresentation();
  const role = useQuery({
    queryKey: queryKeys.factoryRole(reference),
    queryFn: ({ signal }) =>
      api.getRole(reference.project_id, reference.id, reference.version, signal),
  });
  return (
    <section>
      <h5>
        {t('factory.sharedInstruction')} · v{reference.version}
      </h5>
      {role.data ? (
        <InstructionText value={role.data.definition.instructions} />
      ) : role.isPending ? (
        <QueryLoadingNotice label={t('factory.sharedInstruction')} />
      ) : (
        <QueryFailureNotice
          title={t('factory.sharedInstruction')}
          error={role.error}
          onRetry={role.refetch}
        />
      )}
    </section>
  );
}
