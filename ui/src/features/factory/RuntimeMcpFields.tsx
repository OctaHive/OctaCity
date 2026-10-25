import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';

type Server = components['schemas']['FactoryMcpServer'];
type Choice = components['schemas']['FactoryRuntimeChoice'];
const contracts = FACTORY_AUTHORING_SCHEMAS.FactoryMcpServer.oneOf;
/** Choosing a transport never installs an executable or imports ambient settings. */
export function newMcpServer(choice: Choice, transport: Server['transport']): Server {
  return transport === 'http'
    ? { transport, url: '' }
    : {
        transport,
        command: Object.keys(choice.support.mcp_executables)[0] ?? '',
        args: [],
        env_vars: [],
      };
}
export function RuntimeMcpFields({
  server,
  choice,
  onChange,
}: {
  server: Server;
  choice: Choice;
  onChange: (value: Server) => void;
}) {
  const { t } = usePresentation();
  const transports = [...new Set([server.transport, ...choice.support.mcp_transports])];
  return (
    <>
      <SelectMenu
        ariaLabel={t('runtime.transport')}
        value={server.transport}
        options={transports.map((value) => ({
          value,
          label: choice.support.mcp_transports.includes(value)
            ? value
            : `${value} · ${t('runtime.unavailable')}`,
        }))}
        onValueChange={(value) => onChange(newMcpServer(choice, value))}
      />
      {server.transport === 'http' ? (
        <>
          <label>
            {t('runtime.endpoint')}
            <input
              required
              type="url"
              value={server.url}
              maxLength={contracts[0].properties.url.maxLength}
              onChange={(event) => onChange({ ...server, url: event.target.value })}
            />
          </label>
          <SelectMenu
            ariaLabel={t('runtime.bearer')}
            value={server.bearer_token_env_var ?? ''}
            options={[
              { value: '', label: t('factory.none') },
              ...[
                ...new Set([
                  ...(server.bearer_token_env_var ? [server.bearer_token_env_var] : []),
                  ...choice.secret_environment_names,
                ]),
              ].map((value) => ({
                value,
                label: choice.secret_environment_names.includes(value)
                  ? value
                  : `${value} · ${t('runtime.unavailable')}`,
              })),
            ]}
            onValueChange={(value) => {
              const next = { ...server };
              if (value) next.bearer_token_env_var = value;
              else delete next.bearer_token_env_var;
              onChange(next);
            }}
          />
        </>
      ) : (
        <>
          <p>{t('runtime.stdioHelp')}</p>
          <SelectMenu
            ariaLabel={t('factory.executable')}
            value={server.command}
            options={[
              ...new Set([server.command, ...Object.keys(choice.support.mcp_executables)]),
            ].map((value) => ({
              value,
              label: Object.hasOwn(choice.support.mcp_executables, value)
                ? value
                : `${value || t('factory.none')} · ${t('runtime.unavailable')}`,
            }))}
            onValueChange={(command) => onChange({ ...server, command })}
          />
          <RuntimeTextRows
            value={server.args ?? []}
            onChange={(args) => onChange({ ...server, args })}
            label="runtime.argument"
            add="runtime.addArgument"
            remove="runtime.removeArgument"
            maxItems={contracts[1].properties.args.maxItems}
            maxLength={contracts[1].properties.args.items.maxLength}
          />
          <fieldset>
            <legend>{t('factory.environment')}</legend>
            {[...new Set([...(server.env_vars ?? []), ...choice.environment_names])].map((name) => (
              <label key={name}>
                <input
                  type="checkbox"
                  checked={server.env_vars?.includes(name) ?? false}
                  onChange={(event) =>
                    onChange({
                      ...server,
                      env_vars: event.target.checked
                        ? [...(server.env_vars ?? []), name]
                        : (server.env_vars?.filter((item) => item !== name) ?? []),
                    })
                  }
                />
                {name}
                {!choice.environment_names.includes(name) ? ` · ${t('runtime.unavailable')}` : ''}
              </label>
            ))}
          </fieldset>
        </>
      )}
      <label>
        <input
          type="checkbox"
          checked={server.enabled_tools != null}
          onChange={(event) => {
            const next = { ...server };
            if (event.target.checked) next.enabled_tools = [];
            else delete next.enabled_tools;
            onChange(next);
          }}
        />
        {t('runtime.selectTools')}
      </label>
      {server.enabled_tools != null ? (
        <RuntimeTextRows
          value={server.enabled_tools}
          onChange={(enabled_tools) => onChange({ ...server, enabled_tools })}
          label="runtime.toolName"
          add="runtime.addTool"
          remove="runtime.removeTool"
          maxItems={contracts[0].properties.enabled_tools.anyOf[0].maxItems}
          maxLength={contracts[0].properties.enabled_tools.anyOf[0].items.maxLength}
        />
      ) : null}
    </>
  );
}
/** One input is one argv/tool entry; whitespace is data and is never parsed as shell syntax. */
function RuntimeTextRows({
  value,
  onChange,
  label,
  add,
  remove,
  maxItems,
  maxLength,
}: {
  value: string[];
  onChange: (value: string[]) => void;
  label: MessageKey;
  add: MessageKey;
  remove: MessageKey;
  maxItems: number;
  maxLength: number;
}) {
  const { t } = usePresentation();
  const keys = useDraftRowKeys(value);
  return (
    <div>
      {value.map((text, index) => (
        <div key={keys[index]}>
          <label>
            {t(label, { index: index + 1 })}
            <input
              value={text}
              maxLength={maxLength}
              onChange={(event) =>
                onChange(value.map((item, at) => (at === index ? event.target.value : item)))
              }
            />
          </label>
          <button type="button" onClick={() => onChange(value.filter((_, at) => at !== index))}>
            {t(remove, { index: index + 1 })}
          </button>
        </div>
      ))}
      <button
        type="button"
        disabled={value.length >= maxItems}
        onClick={() => onChange([...value, ''])}
      >
        {t(add)}
      </button>
    </div>
  );
}
