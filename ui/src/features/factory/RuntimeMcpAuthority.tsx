import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { McpAuthorizationSummary } from './McpAuthorizationSummary';

type Schema = components['schemas'];
type Entry = Schema['FactoryRuntimeSettingsDefinition']['mcp_servers'][string];
export type RuntimeMcpRow = Entry & { name: string };
type Choice = Schema['FactoryRuntimeChoice'];
type Integrations = Schema['FactoryRuntimeSettingsDefinition']['integrations'];
const referenceKey = (ref: Schema['FactoryImmutableReferenceResource']) => [
  ref.identity,
  ref.version,
  ref.digest,
];

function integrationKey(binding: Integrations[string] | null | undefined) {
  return binding
    ? JSON.stringify([
        binding.connection_id,
        binding.connection_version,
        binding.project_id,
        binding.access_version,
        referenceKey(binding.resource),
        referenceKey(binding.capability),
      ])
    : null;
}

function integrationMatches(
  row: RuntimeMcpRow,
  expected: Integrations[string] | null | undefined,
  integrations: Integrations,
) {
  const current = row.integration ? integrations[row.integration] : null;
  return (!row.integration || !!current) && integrationKey(current) === integrationKey(expected);
}

// Compare named fields rather than JSON member order. Only the server authorizes publication.
function authorityKey(entry: Entry) {
  const { server, authorization: auth } = entry;
  return JSON.stringify([
    server.transport,
    server.transport === 'http'
      ? [server.url, server.bearer_token_env_var ?? null]
      : [server.command, server.args ?? [], server.env_vars ?? []],
    server.enabled_tools ?? null,
    auth
      ? [
          auth.credential_profile,
          auth.secret.provider,
          auth.secret.reference,
          auth.environment,
          auth.purpose,
          referenceKey(auth.resource),
        ]
      : null,
  ]);
}
export function hasInstalledMcpAuthority(
  row: RuntimeMcpRow,
  choice: Choice,
  integrations: Integrations,
) {
  const installed = choice.support.authenticated_mcp?.[row.name];
  return (
    !!installed &&
    authorityKey(row) === authorityKey({ ...installed, integration: null }) &&
    integrationMatches(row, installed.integration, integrations)
  );
}
export function mcpAuthorizationUnavailable(
  row: RuntimeMcpRow,
  choice: Choice,
  integrations: Integrations,
) {
  const needsCredential =
    row.authorization ||
    (row.server.transport === 'http'
      ? row.server.bearer_token_env_var
      : row.server.env_vars?.some((name) => choice.secret_environment_names.includes(name)));
  return !!needsCredential && !hasInstalledMcpAuthority(row, choice, integrations);
}

/** Select an already installed contract; never accept a credential value or invent a grant. */
export function RuntimeMcpAuthority({
  row,
  choice,
  integrations,
  onChange,
}: {
  row: RuntimeMcpRow;
  choice: Choice;
  integrations: Integrations;
  onChange: (patch: Partial<RuntimeMcpRow>) => void;
}) {
  const { t } = usePresentation();
  const installed = choice.support.authenticated_mcp ?? {};
  const selected = installed[row.name];
  const auth = row.authorization;
  return (
    <>
      <SelectMenu
        ariaLabel={t('runtime.installedMcp')}
        value={hasInstalledMcpAuthority(row, choice, integrations) ? row.name : ''}
        options={[
          { value: '', label: t('factory.none') },
          ...Object.keys(installed).map((value) => ({ value, label: value })),
        ]}
        onValueChange={(name) => {
          const selected = installed[name];
          if (!selected) {
            onChange({ authorization: null });
            return;
          }
          const binding = selected.integration;
          const integration = binding
            ? (Object.entries(integrations).find(
                ([, candidate]) => integrationKey(candidate) === integrationKey(binding),
              )?.[0] ?? null)
            : null;
          onChange({
            name,
            server: structuredClone(selected.server),
            authorization: structuredClone(selected.authorization ?? null),
            integration,
          });
        }}
      />
      <p>{t('runtime.authorityHelp')}</p>
      {mcpAuthorizationUnavailable(row, choice, integrations) ? (
        <p role="alert">
          {t(
            selected && !integrationMatches(row, selected.integration, integrations)
              ? 'runtime.integrationUnavailable'
              : 'runtime.authenticationUnavailable',
          )}
        </p>
      ) : null}
      {auth ? <McpAuthorizationSummary authorization={auth} /> : null}
    </>
  );
}
