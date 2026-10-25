import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { FactoryValueInput } from '../../shared/FactoryValueInput';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { InstructionFields, InstructionText } from './InstructionFields';
import { SkillReferences } from './SkillContents';
import { catalogLabels } from './catalogLabels';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';
import type { FactoryRolesApi } from './roleApi';

type Schema = components['schemas'];
type Definition = Schema['FactoryRoleProfileDefinition'];
/** An exact Role pin is adopted explicitly; sibling profiles are never included in this publication. */
export function ProfileEditor({
  api,
  role,
  baseline,
  onDiscard,
}: {
  api: FactoryRolesApi;
  role: Schema['FactoryRole'];
  baseline?: Schema['FactoryRoleProfile'];
  onDiscard: () => void;
}) {
  const { t } = usePresentation();
  const project = role.reference.project_id;
  const client = useQueryClient();
  const [id] = useState(() => baseline?.reference.id ?? crypto.randomUUID());
  const [head, setHead] = useState(baseline?.reference.version ?? null);
  const [rolePin, setRolePin] = useState(baseline?.role ?? role.reference);
  const [definition, setDefinition] = useState<Definition | null>(() =>
    baseline ? structuredClone(baseline.definition) : null,
  );
  const [alias, setAlias] = useState('');
  const [published, setPublished] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [incompatibility, setIncompatibility] = useState<string | null>(null);
  const [review, setReview] = useState<{
    request: Schema['PublishFactoryRoleProfilesRequest'];
    target: HTMLElement;
  } | null>(null);
  const capabilities = useCursorPage(
    queryKeys.factoryRoleCatalog(project, 'capabilities'),
    async (cursor, signal) => {
      const page = await api.listCapabilities(project, cursor, signal);
      return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
    },
  );
  const choices = capabilities.data?.pages.flatMap((page) => page.items) ?? [];
  const selectedAlias =
    alias ||
    choices.find(
      (item) =>
        definition &&
        (['plugin', 'executable', 'model'] as const).every((kind) =>
          (['identity', 'version', 'digest'] as const).every(
            (key) => item.toolchain[kind][key] === definition.toolchain[kind][key],
          ),
        ) &&
        (['project_id', 'id', 'version', 'definition_digest'] as const).every(
          (key) => item.build_configuration[key] === definition.build_configuration[key],
        ),
    )?.alias ||
    '';
  const capability = useQuery({
    queryKey: queryKeys.factoryRoleCatalog(project, 'capability', selectedAlias),
    enabled: !!selectedAlias,
    queryFn: ({ signal }) => api.getCapability(project, selectedAlias, signal),
  });
  const models = capability.data?.model_catalog;
  const model = definition?.model_identifier ?? definition?.toolchain.model.identity ?? '';
  const selectedModel = models?.models.find((choice) => choice.identifier === model);
  async function refreshModels() {
    setPending(true);
    setError(null);
    try {
      const refreshed = await api.refreshCapability(project, selectedAlias);
      client.setQueryData(
        queryKeys.factoryRoleCatalog(project, 'capability', selectedAlias),
        refreshed,
      );
      void client.invalidateQueries({
        queryKey: queryKeys.factoryRoleCatalog(project, 'capabilities'),
      });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  const pinnedRole = useQuery({
    queryKey: queryKeys.factoryRole(rolePin),
    queryFn: ({ signal }) => api.getRole(project, rolePin.id, rolePin.version, signal),
  });
  function update(patch: Partial<Definition>) {
    if (!definition) return;
    const next = { ...definition, ...patch };
    if (patch.model_parameters) {
      const thinking = capability.data?.definition.thinking_parameter;
      next.thinking_parameter =
        thinking && Object.hasOwn(next.model_parameters, thinking) ? thinking : null;
    }
    setDefinition(next);
    return next;
  }
  function selectModel(identifier: string) {
    if (!definition || !capability.data) return;
    const schema =
      models?.models.find((choice) => choice.identifier === identifier)?.parameters ??
      capability.data.parameters;
    const fields = schema.shape.type === 'object' ? schema.shape.fields : {};
    return update({
      model_identifier: identifier,
      model_parameters: Object.fromEntries(
        Object.entries(definition.model_parameters).filter(([name]) => Object.hasOwn(fields, name)),
      ),
    });
  }
  async function validate(target: HTMLElement) {
    if (!definition || !selectedAlias || !capability.data || capability.error) return;
    const reviewed = definition.model_identifier == null ? definition : selectModel(model);
    if (!reviewed) return;
    setPending(true);
    setError(null);
    setIncompatibility(null);
    try {
      const validation = await api.validateProfile(project, {
        id,
        role: rolePin,
        capability: selectedAlias,
        definition: structuredClone(reviewed),
      });
      if (validation.incompatibility || !validation.profile)
        setIncompatibility(validation.incompatibility ?? 'dependencies');
      else
        setReview({
          target,
          request: {
            profiles: [
              {
                id,
                role: rolePin,
                expected_current_version: head,
                definition: validation.profile.definition,
              },
            ],
          },
        });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  return (
    <section aria-label={t('factory.profileEditor')}>
      <h4>{t('factory.profileEditor')}</h4>
      <p>{t('factory.memoryDraft')}</p>
      <button type="button" disabled={pending || !!review} onClick={onDiscard}>
        {t('factory.discardDraft')}
      </button>
      <fieldset disabled={pending || !!review}>
        <PagedExplorerBranch
          query={capabilities}
          empty={t('factory.noProfileRuntimes')}
          labels={catalogLabels(t('factory.profileRuntime'))}
        >
          {() => (
            <SelectMenu
              ariaLabel={t('factory.profileRuntime')}
              value={selectedAlias}
              options={[
                { value: '', label: t('factory.none') },
                ...choices.map((choice) => ({
                  value: choice.alias,
                  label: `${choice.alias} · ${choice.toolchain.plugin.identity} · ${choice.toolchain.model.identity}`,
                })),
              ]}
              onValueChange={async (next) => {
                setAlias(next);
                setPending(true);
                setError(null);
                try {
                  const installed = await api.getCapability(project, next);
                  setDefinition(
                    definition
                      ? {
                          ...definition,
                          toolchain: installed.definition.toolchain,
                          build_configuration: installed.definition.build_configuration,
                          thinking_parameter: installed.definition.thinking_parameter ?? null,
                        }
                      : {
                          ...structuredClone(installed.definition),
                          name: '',
                          model_identifier: installed.model_catalog
                            ? ''
                            : (installed.definition.model_identifier ?? null),
                          instructions: [],
                          skills: {},
                        },
                  );
                } catch (failure) {
                  setError(failure instanceof Error ? failure : new Error());
                } finally {
                  setPending(false);
                }
              }}
            />
          )}
        </PagedExplorerBranch>
        {capability.error ? (
          <QueryFailureNotice
            title={t('factory.profileRuntime')}
            error={capability.error}
            onRetry={capability.refetch}
          />
        ) : null}
        {pinnedRole.data ? (
          <>
            <h5>
              {t('factory.sharedInstruction')} · v{rolePin.version}
            </h5>
            <InstructionText value={pinnedRole.data.definition.instructions} />
          </>
        ) : pinnedRole.isPending ? (
          <QueryLoadingNotice label={t('factory.sharedInstruction')} />
        ) : (
          <QueryFailureNotice
            title={t('factory.sharedInstruction')}
            error={pinnedRole.error}
            onRetry={pinnedRole.refetch}
          />
        )}
        {rolePin.digest !== role.reference.digest ? (
          <button type="button" onClick={() => setRolePin(role.reference)}>
            {t('factory.adoptRole', { version: role.reference.version })}
          </button>
        ) : null}
        {definition ? (
          <>
            <label>
              {t('factory.profileName')}
              <input
                value={definition.name}
                onChange={(event) => update({ name: event.target.value })}
              />
            </label>
            {models ? (
              <>
                <SelectMenu
                  ariaLabel={t('factory.modelSuggestions')}
                  value={selectedModel?.identifier ?? ''}
                  options={[
                    { value: '', label: t('factory.none') },
                    ...models.models.map((choice) => ({
                      value: choice.identifier,
                      label: choice.label,
                    })),
                  ]}
                  onValueChange={(value) => {
                    if (value) selectModel(value);
                  }}
                />
                <label>
                  {t('factory.modelIdentifier')}
                  <input
                    value={model}
                    readOnly={!models.manual_model}
                    onChange={(event) => update({ model_identifier: event.target.value })}
                  />
                </label>
                <p>
                  {t(
                    models.available
                      ? 'factory.modelSuggestionsHint'
                      : 'factory.modelDiscoveryUnavailable',
                  )}
                </p>
                <button type="button" onClick={() => void refreshModels()}>
                  {t('factory.refreshModels')}
                </button>
              </>
            ) : null}
            <InstructionFields
              label={t('factory.profileInstruction')}
              value={definition.instructions}
              onChange={(instructions) => update({ instructions })}
            />
            {capability.data && !capability.error ? (
              <button
                type="button"
                onClick={() =>
                  setDefinition({
                    ...structuredClone(capability.data!.definition),
                    name: definition.name,
                    instructions: definition.instructions,
                    skills: definition.skills,
                  })
                }
              >
                {t('factory.useInstalledSettings')}
              </button>
            ) : null}
            <SkillReferences
              api={api}
              project={project}
              skills={pinnedRole.data?.definition.skills ?? {}}
            />
            {Object.keys(definition.skills).length ? (
              <SkillReferences api={api} project={project} skills={definition.skills} />
            ) : null}
            <p>
              {t('factory.thinking')} · {definition.thinking_parameter ?? t('factory.none')}
            </p>
            {capability.data && !capability.error ? (
              <>
                <FactoryValueInput
                  schema={(selectedModel?.parameters ?? capability.data.parameters).shape}
                  value={definition.model_parameters}
                  label={t('factory.modelParameters')}
                  onChange={(value) =>
                    update({ model_parameters: value as Definition['model_parameters'] })
                  }
                />
                <ProfileSettings
                  value={definition}
                  ceiling={capability.data.definition}
                  environment={capability.data.public_environment}
                  onChange={setDefinition}
                />
              </>
            ) : null}
            <button
              type="button"
              disabled={
                !definition.name.trim() ||
                (!!models && !model.trim()) ||
                !capability.data ||
                !!capability.error ||
                !pinnedRole.data ||
                !!pinnedRole.error
              }
              onClick={(event) => void validate(event.currentTarget)}
            >
              {t('factory.reviewProfile')}
            </button>
          </>
        ) : null}
      </fieldset>
      {incompatibility ? (
        <p role="alert">{t('factory.incompatibleProfile', { reason: incompatibility })}</p>
      ) : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.profileEditor')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
      {review ? (
        <ConfirmedCommand
          request={review.request}
          title={t('factory.reviewProfile')}
          confirmLabel={t('factory.confirmPublish')}
          consequence={`${definition?.name} · ${t('factory.publishConsequence')} · ${rolePin.id} v${rolePin.version}`}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryRoleCatalog(project, 'profiles', role.reference.id) },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const result = await api.publish(
              project,
              structuredClone(attempt.request) as Schema['PublishFactoryRoleProfilesRequest'],
              attempt.headers,
            );
            setPublished(true);
            return result;
          }}
          onClose={() => (published ? onDiscard() : setReview(null))}
          onRefresh={async () => {
            try {
              const current = await api.currentProfile(project, id);
              setHead(current.reference.version);
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            }
          }}
          renderSuccess={() => <p>{t('factory.published')}</p>}
        />
      ) : null}
    </section>
  );
}
function ProfileSettings({
  value,
  ceiling,
  environment,
  onChange,
}: {
  value: Definition;
  ceiling: Definition;
  environment: Schema['FactoryRoleProfileCapabilityDetails']['public_environment'];
  onChange: (value: Definition) => void;
}) {
  const { t } = usePresentation();
  const execution = value.execution ?? {};
  const argumentKeys = useDraftRowKeys(ceiling.execution?.cli_arguments ?? []);
  const setExecution = (patch: Partial<Schema['FactoryRoleProfileExecution']>) =>
    onChange({ ...value, execution: { ...execution, ...patch } });
  const hasEnvironment =
    Object.keys(environment).length > 0 ||
    Object.values(ceiling.execution?.environment ?? {}).some((entry) => entry.kind === 'secret');
  return (
    <>
      <p>{t('factory.runtimeSelectionHelp')}</p>
      {hasEnvironment ? (
        <fieldset>
          <legend>{t('factory.environment')}</legend>
          {Object.entries(environment).map(([name, schema]) => (
            <FactoryValueInput
              key={name}
              schema={schema.shape}
              label={name}
              value={
                execution.environment?.[name]?.kind === 'public'
                  ? execution.environment[name].value
                  : ''
              }
              onChange={(next) =>
                setExecution({
                  environment: {
                    ...execution.environment,
                    [name]: { kind: 'public', value: String(next) },
                  },
                })
              }
            />
          ))}
          {Object.entries(ceiling.execution?.environment ?? {})
            .filter(([, entry]) => entry.kind === 'secret')
            .map(([name, entry]) => (
              <label key={name}>
                <input
                  type="checkbox"
                  checked={!!execution.environment?.[name]}
                  onChange={(event) => {
                    const next = { ...execution.environment };
                    if (event.target.checked) next[name] = entry;
                    else delete next[name];
                    setExecution({ environment: next });
                  }}
                />
                {name} · {t('factory.secretReference')}
              </label>
            ))}
        </fieldset>
      ) : null}
      {(value.execution?.cli_arguments?.length ?? 0) > 0 ? (
        <fieldset>
          <legend>{t('factory.cliArguments')}</legend>
          <p>{t('factory.installedArguments')}</p>
          <ol>
            {(value.execution?.cli_arguments ?? []).map((argument, index) => (
              <li key={argumentKeys[index]}>{argument}</li>
            ))}
          </ol>
        </fieldset>
      ) : null}
      {!hasEnvironment && !value.execution?.cli_arguments?.length ? (
        <p>{t('factory.noRuntimeOverrides')}</p>
      ) : null}
      {(['mcp_servers', 'integrations'] as const).map((field) =>
        Object.keys(ceiling.execution?.[field] ?? {}).length ? (
          <fieldset key={field}>
            <legend>{t(field === 'mcp_servers' ? 'factory.mcp' : 'factory.integrations')}</legend>
            {Object.entries(ceiling.execution?.[field] ?? {}).map(([name, entry]) => (
              <label key={name}>
                <input
                  type="checkbox"
                  checked={!!execution[field]?.[name]}
                  onChange={(event) => {
                    const next: Record<
                      string,
                      Schema['FactoryRoleMcpBinding'] | Schema['FactoryRoleIntegrationBinding']
                    > = { ...execution[field] };
                    if (event.target.checked) next[name] = entry;
                    else delete next[name];
                    setExecution({ [field]: next });
                  }}
                />
                {name}
                {'configuration' in entry && entry.integration ? (
                  <>
                    {' '}
                    · <span>{t('factory.mcpIntegration', { integration: entry.integration })}</span>
                  </>
                ) : null}
              </label>
            ))}
          </fieldset>
        ) : (
          <section key={field}>
            <h4>{t(field === 'mcp_servers' ? 'factory.mcp' : 'factory.integrations')}</h4>
            <p>
              {t(
                field === 'mcp_servers' ? 'factory.noRuntimeMcp' : 'factory.noRuntimeIntegrations',
              )}
            </p>
          </section>
        ),
      )}
      <BudgetEditor
        label={t('factory.profileBudget')}
        value={value.budget}
        ceiling={ceiling.budget}
        onChange={(budget) => onChange({ ...value, budget })}
      />
      <PermissionEditor
        label={t('factory.nodePermissions')}
        value={value.permissions}
        ceiling={ceiling.permissions}
        choices={ceiling.permissions}
        onChange={(permissions) => onChange({ ...value, permissions })}
      />
    </>
  );
}
