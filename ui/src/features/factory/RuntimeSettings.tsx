import { useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { aiSettingsPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { RuntimeIntegrations, type RuntimeIntegrationApi } from './RuntimeIntegrations';
import { SelectMenu } from '../../shared/SelectMenu';
import { RuntimeMcpFields, newMcpServer } from './RuntimeMcpFields';
import { RuntimeSetupGuidance } from './RuntimeSetupGuidance';
import {
  RuntimeMcpAuthority,
  mcpAuthorizationUnavailable,
  type RuntimeMcpRow,
} from './RuntimeMcpAuthority';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { catalogLabels } from './catalogLabels';
import { PermissionEditor } from './ResourceEditors';
import { factoryRuntimeApi } from './runtimeApi';

type Schema = components['schemas'];
type Choice = Schema['FactoryRuntimeChoice'];
type Settings = Schema['FactoryRuntimeSettings'];
type Request = Schema['FactoryRuntimeSettingsRequest'];
export type RuntimeApi = typeof factoryRuntimeApi;
type McpRow = RuntimeMcpRow;

export function RuntimeSettings({
  project,
  api,
  integrations,
}: {
  project: string;
  api: RuntimeApi;
  integrations: RuntimeIntegrationApi;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<Choice | null>(null);
  const choices = useCursorPage(queryKeys.factoryRuntimeChoices(project), async (after, signal) => {
    const page = await api.choices(project, after ? { after } : {}, signal);
    return { ...page, items: page.items.map((item) => ({ ...item, id: item.alias })) };
  });
  return (
    <section aria-label={t('runtime.runtimes')}>
      <p>{t('runtime.help')}</p>
      <p>{t('runtime.profileHelp')}</p>
      <Link to={aiSettingsPath(project, 'agents')}>{t('runtime.configureProfile')}</Link>
      <PagedExplorerBranch
        query={choices}
        empty={t('runtime.noInstalled')}
        labels={catalogLabels(t('runtime.runtimes'))}
      >
        {(items) => (
          <ul>
            {items.map((item) => (
              <li key={item.alias}>
                <button
                  type="button"
                  aria-pressed={selected?.alias === item.alias}
                  onClick={() => setSelected(item)}
                >
                  {item.alias}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {selected ? (
        <RuntimeDetails
          key={selected.alias}
          project={project}
          choice={selected}
          api={api}
          integrations={integrations}
        />
      ) : null}
    </section>
  );
}
function RuntimeDetails({
  project,
  choice,
  api,
  integrations,
}: {
  project: string;
  choice: Choice;
  api: RuntimeApi;
  integrations: RuntimeIntegrationApi;
}) {
  const { t } = usePresentation();
  const current = useQuery({
    queryKey: queryKeys.factoryRuntime(project, choice.alias),
    queryFn: ({ signal }) => api.current(project, choice.alias, signal),
  });
  if (current.isPending) return <QueryLoadingNotice label={t('runtime.runtimes')} />;
  if (!current.data)
    return (
      <QueryFailureNotice
        title={t('runtime.runtimes')}
        error={current.error}
        onRetry={current.refetch}
      />
    );
  return (
    <RuntimeEditor
      project={project}
      choice={choice}
      baseline={current.data.settings}
      api={api}
      integrations={integrations}
    />
  );
}
function RuntimeEditor({
  project,
  choice,
  baseline,
  api,
  integrations,
}: {
  project: string;
  choice: Choice;
  baseline: Settings | null;
  api: RuntimeApi;
  integrations: RuntimeIntegrationApi;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [definition, setDefinition] = useState(() =>
    structuredClone(
      baseline?.definition ?? {
        toolchain: choice.toolchain,
        mcp_servers: {},
        integrations: {},
        permissions: choice.permissions,
      },
    ),
  );
  const [rows, setRows] = useState<McpRow[]>(() =>
    Object.entries(definition.mcp_servers).map(([name, entry]) => ({ name, ...entry })),
  );
  const keys = useDraftRowKeys(rows);
  const [head, setHead] = useState(baseline?.version ?? null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{ request: Request; target: HTMLElement } | null>(null);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const update = (index: number, patch: Partial<McpRow>) =>
    setRows(rows.map((row, at) => (at === index ? { ...row, ...patch } : row)));
  const installationChanged = (['plugin', 'executable', 'model'] as const).some((kind) =>
    (['identity', 'version', 'digest'] as const).some(
      (field) => definition.toolchain[kind][field] !== choice.toolchain[kind][field],
    ),
  );
  const namesUnique = new Set(rows.map((row) => row.name)).size === rows.length;
  const authorizationUnavailable = rows.some((row) =>
    mcpAuthorizationUnavailable(row, choice, definition.integrations),
  );
  async function validate(target: HTMLElement) {
    if (!namesUnique || installationChanged || authorizationUnavailable) return;
    setPending(true);
    setError(null);
    const request: Request = {
      expected_version: head,
      definition: {
        ...structuredClone(definition),
        mcp_servers: Object.fromEntries(
          rows.map(({ name, ...entry }) => [name, structuredClone(entry)]),
        ),
      },
    };
    try {
      const checked = await api.validate(project, choice.alias, request);
      setReview({ target, request: { ...request, definition: checked.settings.definition } });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  const bounds = FACTORY_AUTHORING_SCHEMAS.FactoryRuntimeSettingsDefinition.properties.mcp_servers;
  return (
    <section aria-label={t('runtime.editor')}>
      <h3>{choice.alias}</h3>
      <p>{t('factory.memoryDraft')}</p>
      <p>{t('runtime.permissionsHelp')}</p>
      <RuntimeSetupGuidance choice={choice} />
      {!choice.support.mcp_transports.length ? <p>{t('runtime.unsupportedMcp')}</p> : null}
      {installationChanged ? (
        <div>
          <p>{t('runtime.installationChanged')}</p>
          <button
            type="button"
            disabled={pending || review !== null}
            onClick={() =>
              setDefinition({ ...definition, toolchain: structuredClone(choice.toolchain) })
            }
          >
            {t('runtime.adoptInstallation')}
          </button>
        </div>
      ) : null}

      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (reviewButtonRef.current) void validate(reviewButtonRef.current);
        }}
      >
        <fieldset disabled={pending || review !== null}>
          <RuntimeIntegrations
            project={project}
            api={integrations}
            value={definition.integrations}
            onChange={(value) => setDefinition({ ...definition, integrations: value })}
          />
          {rows.map((row, index) => (
            <fieldset key={keys[index]}>
              <legend>{t('runtime.mcpServer', { index: index + 1 })}</legend>
              <label>
                {t('runtime.mcpName')}
                <input
                  required
                  pattern={bounds.propertyNames.pattern}
                  maxLength={bounds.propertyNames.maxLength}
                  value={row.name}
                  onChange={(event) => update(index, { name: event.target.value })}
                />
              </label>
              <SelectMenu
                ariaLabel={t('runtime.requiredIntegration')}
                value={row.integration ?? ''}
                options={[
                  { value: '', label: t('factory.none') },
                  ...Object.keys(definition.integrations).map((value) => ({ value, label: value })),
                  ...(row.integration && !Object.hasOwn(definition.integrations, row.integration)
                    ? [
                        {
                          value: row.integration,
                          label: `${row.integration} · ${t('runtime.unavailable')}`,
                        },
                      ]
                    : []),
                ]}
                onValueChange={(integration) => update(index, { integration: integration || null })}
              />
              <RuntimeMcpFields
                server={row.server}
                choice={choice}
                onChange={(server) => update(index, { server })}
              />
              <RuntimeMcpAuthority
                row={row}
                choice={choice}
                integrations={definition.integrations}
                onChange={(patch) => update(index, patch)}
              />
              <button type="button" onClick={() => setRows(rows.filter((_, at) => at !== index))}>
                {t('runtime.removeMcp', { index: index + 1 })}
              </button>
            </fieldset>
          ))}
          {!rows.length ? <p>{t('runtime.noMcp')}</p> : null}
          <button
            type="button"
            disabled={
              choice.support.mcp_transports.length === 0 || rows.length >= bounds.maxProperties
            }
            onClick={() =>
              setRows([
                ...rows,
                {
                  name: '',
                  server: newMcpServer(choice, choice.support.mcp_transports[0]!),
                  integration: null,
                },
              ])
            }
          >
            {t('runtime.addMcp')}
          </button>
          {!namesUnique ? <p role="alert">{t('runtime.uniqueNames')}</p> : null}
          <PermissionEditor
            label={t('factory.permissionCeiling')}
            value={definition.permissions}
            choices={choice.permissions}
            ceiling={choice.permissions}
            enforcement={choice.enforcement}
            onChange={(permissions) => setDefinition({ ...definition, permissions })}
          />
          <button
            ref={reviewButtonRef}
            type="submit"
            disabled={!namesUnique || installationChanged || authorizationUnavailable}
          >
            {t('runtime.review')}
          </button>
        </fieldset>
      </form>
      {error ? (
        <QueryFailureNotice
          title={t('runtime.review')}
          error={error}
          onRetry={() => reviewButtonRef.current && void validate(reviewButtonRef.current)}
        />
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('runtime.review')}
          confirmLabel={t('runtime.publish')}
          consequence={t('runtime.publishConsequence', { alias: choice.alias })}
          request={review.request}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryRuntime(project, choice.alias) },
            { queryKey: queryKeys.factoryRoleCatalog(project, 'capabilities') },
            { queryKey: queryKeys.factoryChoices(project) },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const result = await api.publish(
              project,
              choice.alias,
              structuredClone(attempt.request) as Request,
              attempt.headers,
            );
            setHead(result.settings.version);
            return result;
          }}
          onRefresh={async () => {
            try {
              const latest = await api.current(project, choice.alias);
              setHead(latest.settings?.version ?? null);
              client.setQueryData(queryKeys.factoryRuntime(project, choice.alias), latest);
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            }
          }}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('runtime.published')}</p>}
        />
      ) : null}
    </section>
  );
}
