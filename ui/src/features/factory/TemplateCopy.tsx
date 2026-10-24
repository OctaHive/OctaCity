import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { SelectMenu } from '../../shared/SelectMenu';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { AgentFields } from './AgentFields';
import { InstructionFields } from './InstructionFields';
import { catalogLabels } from './catalogLabels';
import type { FactoryRolesApi } from './roleApi';

type Schema = components['schemas'];
/** Templates are copied once into ordinary independently identified Role/Profile definitions. */
export function TemplateCopy({
  api,
  project,
  reference,
  onDiscard,
}: {
  api: FactoryRolesApi;
  project: string;
  reference: Schema['FactoryImmutableReferenceResource'];
  onDiscard: () => void;
}) {
  const { t } = usePresentation();
  const template = useQuery({
    queryKey: queryKeys.factoryRoleCatalog(project, 'template', reference.digest),
    queryFn: ({ signal }) => api.getTemplate(project, reference, signal),
  });
  if (!template.data)
    return template.isPending ? (
      <QueryLoadingNotice label={t('factory.copyTemplate')} />
    ) : (
      <QueryFailureNotice
        title={t('factory.copyTemplate')}
        error={template.error}
        onRetry={template.refetch}
      />
    );
  return (
    <CopyDraft
      api={api}
      project={project}
      reference={reference}
      template={template.data}
      onDiscard={onDiscard}
    />
  );
}
function CopyDraft({
  api,
  project,
  reference,
  template,
  onDiscard,
}: {
  api: FactoryRolesApi;
  project: string;
  reference: Schema['FactoryImmutableReferenceResource'];
  template: Schema['FactoryRoleTemplate'];
  onDiscard: () => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [id] = useState(() => crypto.randomUUID());
  const [role, setRole] = useState(() => structuredClone(template.role));
  const [aliases, setAliases] = useState<Record<string, string>>({});
  const [profiles, setProfiles] = useState<Schema['CopyFactoryRoleTemplateRequest']['profiles']>(
    {},
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{
    target: HTMLElement;
    request: Schema['CopyFactoryRoleTemplateRequest'];
  } | null>(null);
  const capabilities = useCursorPage(
    queryKeys.factoryRoleCatalog(project, 'capabilities'),
    async (cursor, signal) => {
      const page = await api.listCapabilities(project, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
    },
  );
  const choices = capabilities.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <section aria-label={t('factory.copyTemplate')}>
      <h3>{t('factory.copyTemplate')}</h3>
      <p>{t('factory.memoryDraft')}</p>
      <button type="button" disabled={busy || !!review} onClick={onDiscard}>
        {t('factory.discardDraft')}
      </button>
      <fieldset disabled={busy || !!review}>
        <AgentFields value={role} onChange={setRole} />
        <PagedExplorerBranch
          query={capabilities}
          empty={t('factory.noChoices')}
          labels={catalogLabels(t('factory.profileRuntime'))}
        >
          {() => null}
        </PagedExplorerBranch>
        {Object.entries(template.profiles).map(([alias, seed]) => (
          <fieldset key={alias}>
            <legend>{alias}</legend>
            <SelectMenu
              ariaLabel={`${t('factory.profileRuntime')} · ${alias}`}
              value={aliases[alias] ?? ''}
              options={[
                { value: '', label: t('factory.none') },
                ...choices.map((choice) => ({
                  value: choice.alias,
                  label: `${choice.alias} · ${choice.toolchain.plugin.identity} · ${choice.toolchain.model.identity}`,
                })),
              ]}
              onValueChange={async (selected) => {
                setBusy(true);
                setError(null);
                try {
                  const capability = await api.getCapability(project, selected);
                  setAliases((current) => ({ ...current, [alias]: selected }));
                  setProfiles((current) => ({
                    ...current,
                    [alias]: {
                      id: current[alias]?.id ?? crypto.randomUUID(),
                      definition: {
                        ...structuredClone(capability.definition),
                        ...seed,
                        ...(current[alias]
                          ? {
                              instructions: current[alias].definition.instructions,
                              name: current[alias].definition.name,
                            }
                          : {}),
                      },
                    },
                  }));
                } catch (failure) {
                  setError(failure instanceof Error ? failure : new Error());
                } finally {
                  setBusy(false);
                }
              }}
            />
            {profiles[alias] ? (
              <>
                <label>
                  {t('factory.profileName')} · {alias}
                  <input
                    value={profiles[alias].definition.name}
                    onChange={(event) =>
                      setProfiles({
                        ...profiles,
                        [alias]: {
                          ...profiles[alias]!,
                          definition: { ...profiles[alias]!.definition, name: event.target.value },
                        },
                      })
                    }
                  />
                </label>
                <InstructionFields
                  value={profiles[alias].definition.instructions}
                  label={`${t('factory.profileInstruction')} · ${alias}`}
                  onChange={(instructions) =>
                    setProfiles({
                      ...profiles,
                      [alias]: {
                        ...profiles[alias]!,
                        definition: { ...profiles[alias]!.definition, instructions },
                      },
                    })
                  }
                />
              </>
            ) : null}
          </fieldset>
        ))}
        <button
          type="button"
          disabled={
            !role.name.trim() ||
            Object.keys(template.profiles).some((alias) => !profiles[alias]) ||
            !!capabilities.error
          }
          onClick={async (event) => {
            const target = event.currentTarget;
            setBusy(true);
            setError(null);
            try {
              const normalized = await api.validateRole(project, {
                id,
                definition: structuredClone(role),
              });
              setReview({
                target,
                request: {
                  role_id: id,
                  role: normalized.definition,
                  profiles: structuredClone(profiles),
                  template: reference,
                },
              });
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            } finally {
              setBusy(false);
            }
          }}
        >
          {t('factory.reviewAgent')}
        </button>
      </fieldset>
      {error ? (
        <QueryFailureNotice
          title={t('factory.copyTemplate')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('factory.reviewAgent')}
          consequence={`${review.request.role.name} · ${Object.values(review.request.profiles)
            .map((profile) => profile.definition.name)
            .join(' · ')} · ${t('factory.publishConsequence')}`}
          request={review.request}
          confirmLabel={t('factory.confirmPublish')}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryRoleCatalog(project, 'roles') },
            { queryKey: queryKeys.audit },
          ]}
          execute={(attempt) =>
            api.copyTemplate(
              project,
              structuredClone(attempt.request) as Schema['CopyFactoryRoleTemplateRequest'],
              attempt.headers,
            )
          }
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('factory.published')}</p>}
        />
      ) : null}
    </section>
  );
}
