import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

type Schema = components['schemas'];
type Execution = Schema['FactoryRoleProfileExecution'];
type Binding = Schema['FactoryRoleMcpBinding'] | Schema['FactoryRoleIntegrationBinding'];
const referenceKey = (ref: Schema['FactoryImmutableReferenceResource']) => [
  ref.identity,
  ref.version,
  ref.digest,
];
/** Compare exact identities independent of JSON member order; aliases alone never adopt a revision. */
function bindingKey(binding: Binding | undefined) {
  if (!binding) return '';
  return JSON.stringify(
    'configuration' in binding
      ? [referenceKey(binding.configuration), binding.integration ?? null]
      : [
          binding.connection_id,
          binding.connection_version,
          binding.project_id,
          binding.access_version,
          referenceKey(binding.resource),
          referenceKey(binding.capability),
        ],
  );
}

export function ProfileBindings({
  value,
  choices,
  onChange,
}: {
  value: Execution;
  choices: Execution;
  onChange: (patch: Partial<Execution>) => void;
}) {
  const { t } = usePresentation();
  return (['mcp_servers', 'integrations'] as const).map((field) => {
    const available = choices[field] ?? {};
    const selected = value[field] ?? {};
    const stale = Object.entries(selected).filter(
      ([name, entry]) => bindingKey(entry) !== bindingKey(available[name]),
    );
    function select(name: string, entry?: Binding) {
      const next: Record<string, Binding> = { ...selected };
      if (entry) next[name] = entry;
      else delete next[name];
      onChange({ [field]: next });
    }
    return (
      <section key={field}>
        {Object.keys(available).length ? (
          <fieldset>
            <legend>{t(field === 'mcp_servers' ? 'factory.mcp' : 'factory.integrations')}</legend>
            {Object.entries(available).map(([name, entry]) => (
              <label key={name}>
                <input
                  type="checkbox"
                  checked={bindingKey(selected[name]) === bindingKey(entry)}
                  onChange={(event) => select(name, event.target.checked ? entry : undefined)}
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
          <>
            <h4>{t(field === 'mcp_servers' ? 'factory.mcp' : 'factory.integrations')}</h4>
            <p>
              {t(
                field === 'mcp_servers' ? 'factory.noRuntimeMcp' : 'factory.noRuntimeIntegrations',
              )}
            </p>
          </>
        )}
        {field === 'mcp_servers'
          ? Object.entries(value.mcp_servers ?? {}).map(([name, entry]) =>
              entry.integration &&
              (!value.integrations?.[entry.integration] ||
                bindingKey(value.integrations[entry.integration]) !==
                  bindingKey(choices.integrations?.[entry.integration])) ? (
                <p key={name}>
                  {name} · {t('factory.selectMcpIntegration', { integration: entry.integration })}
                </p>
              ) : null,
            )
          : null}
        {stale.map(([name]) => (
          <div key={name}>
            <p>
              {name} · {t('factory.retainedBinding')}
            </p>
            <button type="button" onClick={() => select(name)}>
              {t('factory.removeBinding', { name })}
            </button>
          </div>
        ))}
      </section>
    );
  });
}
