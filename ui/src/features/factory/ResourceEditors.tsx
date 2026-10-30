import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import styles from './Factory.module.css';

type Budget = components['schemas']['FactoryBudget'];
type Permissions = components['schemas']['FactoryPermissions'];
type Choice = components['schemas']['FactoryConfigurationChoiceResource'];
type ReferenceField = 'plugins' | 'executables' | 'tools';
type LogicalField = 'secret_profiles' | 'workload_identity_profiles';
type PermissionCategory = components['schemas']['FactoryRuntimeChoice']['enforcement'][number];
const CATEGORY = {
  plugins: 'plugin_identity',
  executables: 'executable_identity',
  tools: 'tool_identity',
  secret_profiles: 'secret_profiles',
  workload_identity_profiles: 'workload_identity_profiles',
} as const;
const RESOURCE_CATEGORY = {
  cpu_millis: 'cpu',
  memory_bytes: 'memory',
  disk_bytes: 'disk',
  elapsed_millis: 'elapsed_time',
  process_count: 'process_count',
} as const;
type PermissionChoices = readonly Choice[] | Pick<Permissions, ReferenceField | LogicalField>;

/** Keep unavailable selected values removable without offering them for new selection. */
function permissionOptions<T>(
  available: readonly T[],
  selected: readonly T[],
  key: (value: T) => string = JSON.stringify,
) {
  const offered = new Set(available.map(key));
  return [
    ...available.map((value) => ({ value, retained: false })),
    ...selected
      .filter((value) => !offered.has(key(value)))
      .map((value) => ({ value, retained: true })),
  ];
}
const referenceKey = (reference: Permissions['tools'][number]) =>
  JSON.stringify([reference.identity, reference.version, reference.digest]);

const BUDGET_LABELS = {
  max_attempts: 'factory.maxAttempts',
  max_elapsed_millis: 'factory.maxElapsed',
  max_tokens: 'factory.maxTokens',
  max_cost_micro_units: 'factory.maxCost',
  max_output_bytes: 'factory.maxOutput',
} satisfies Record<keyof Budget, MessageKey>;

export function BudgetEditor({
  value,
  onChange,
  label,
  ceiling,
}: {
  value: Budget;
  onChange: (value: Budget) => void;
  label: string;
  ceiling?: Budget;
}) {
  const { t } = usePresentation();
  return (
    <fieldset>
      <legend>{label}</legend>
      <div className={styles.numericGrid}>
        {(Object.keys(BUDGET_LABELS) as (keyof Budget)[]).map((key) => (
          <label key={key}>
            {label} · {t(BUDGET_LABELS[key])}
            <input
              type="number"
              min={0}
              max={ceiling?.[key] ?? Number.MAX_SAFE_INTEGER}
              value={value[key]}
              onChange={(event) => onChange({ ...value, [key]: Number(event.target.value) })}
            />
          </label>
        ))}
      </div>
    </fieldset>
  );
}

export function PermissionEditor({
  value,
  choices,
  onChange,
  label,
  ceiling,
  enforcement,
}: {
  value: Permissions;
  ceiling?: Permissions;
  enforcement?: readonly PermissionCategory[];
  choices: PermissionChoices;
  onChange: (value: Permissions) => void;
  label: string;
}) {
  const { t } = usePresentation();
  const supported = (...categories: PermissionCategory[]) =>
    !enforcement || categories.every((category) => enforcement.includes(category));
  const unavailable = (allowed: boolean) =>
    allowed ? null : <p>{t('factory.permissionUnsupported')}</p>;
  const toggle = (
    field: ReferenceField,
    reference: Permissions['tools'][number],
    enabled: boolean,
  ) => {
    const remaining = value[field].filter(
      (item) =>
        item.digest !== reference.digest ||
        item.identity !== reference.identity ||
        item.version !== reference.version,
    );
    onChange({ ...value, [field]: enabled ? [...remaining, reference] : remaining });
  };
  // Profile capabilities already separate permission categories; ordinary Flow catalogs
  // retain their existing generic node-capability choices.
  const retainedHint = (retained: boolean) =>
    retained ? <span> · {t('factory.retainedPermission')}</span> : null;
  const referenceChoices = (field: ReferenceField) =>
    permissionOptions(
      'plugins' in choices
        ? choices[field].map((reference) => ({ alias: reference.identity, reference }))
        : choices.flatMap((choice) =>
            choice.kind === 'node_capability' && choice.reference.kind === 'immutable'
              ? [{ alias: choice.alias, reference: choice.reference.reference }]
              : [],
          ),
      value[field].map((reference) => ({ alias: reference.identity, reference })),
      (item) => referenceKey(item.reference),
    );
  const logicalChoices = (field: LogicalField) =>
    permissionOptions(
      'plugins' in choices
        ? choices[field]
        : choices
            .filter(
              (choice) =>
                choice.kind ===
                (field === 'secret_profiles' ? 'secret_profile' : 'workload_identity_profile'),
            )
            .map((choice) => choice.alias),
      value[field],
    );
  const hosts = permissionOptions(ceiling?.network_hosts ?? [], value.network_hosts);
  const mounts = permissionOptions(ceiling?.mounts ?? [], value.mounts, (mount) =>
    JSON.stringify([mount.root, mount.mode]),
  );
  const commands = permissionOptions(ceiling?.commands ?? [], value.commands);
  const outputKinds = permissionOptions(ceiling?.outputs.kinds ?? [], value.outputs.kinds);
  const resourceFields = (
    Object.keys(value.resources) as (keyof Permissions['resources'])[]
  ).filter((key) =>
    supported(RESOURCE_CATEGORY[key])
      ? !ceiling || ceiling.resources[key] > 0 || value.resources[key] > 0
      : value.resources[key] > 0,
  );
  const outputFields = (Object.keys(value.outputs) as (keyof Permissions['outputs'])[]).filter(
    (key) =>
      key !== 'kinds' &&
      (supported('outputs')
        ? !ceiling || Number(ceiling.outputs[key]) > 0 || Number(value.outputs[key]) > 0
        : Number(value.outputs[key]) > 0),
  );
  const descendantsEditable = supported('descendant_processes')
    ? !ceiling || ceiling.max_descendants > 0 || value.max_descendants > 0
    : value.max_descendants > 0;
  const hasSelections =
    (['plugins', 'executables', 'tools'] as const).some(
      (field) => referenceChoices(field).length > 0,
    ) ||
    (['secret_profiles', 'workload_identity_profiles'] as const).some(
      (field) => logicalChoices(field).length > 0,
    ) ||
    [hosts, mounts, commands, outputKinds].some((options) => options.length > 0);
  if (!hasSelections && !resourceFields.length && !outputFields.length && !descendantsEditable) {
    return (
      <section>
        <h4>{label}</h4>
        <p>{t('factory.noRuntimePermissions')}</p>
      </section>
    );
  }
  return (
    <fieldset>
      <legend>{label}</legend>
      {(['plugins', 'executables', 'tools'] as const).map((field) =>
        referenceChoices(field).length ? (
          <fieldset key={field}>
            <legend>
              {t(
                field === 'plugins'
                  ? 'factory.plugin'
                  : field === 'tools'
                    ? 'factory.tool'
                    : 'factory.executable',
              )}
            </legend>
            {unavailable(supported(CATEGORY[field]))}
            {referenceChoices(field).map(({ value: { alias, reference }, retained }) => {
              return (
                <label
                  key={`${alias}:${reference.identity}:${reference.version}:${reference.digest}`}
                >
                  <input
                    type="checkbox"
                    disabled={
                      !supported(CATEGORY[field]) &&
                      !value[field].some(
                        (item) =>
                          item.digest === reference.digest &&
                          item.identity === reference.identity &&
                          item.version === reference.version,
                      )
                    }
                    checked={value[field].some(
                      (item) =>
                        item.digest === reference.digest &&
                        item.identity === reference.identity &&
                        item.version === reference.version,
                    )}
                    onChange={(event) => toggle(field, reference, event.target.checked)}
                  />
                  {t(
                    field === 'plugins'
                      ? 'factory.plugin'
                      : field === 'tools'
                        ? 'factory.tool'
                        : 'factory.executable',
                  )}{' '}
                  · {alias} · {reference.version}
                  {retainedHint(retained)}
                </label>
              );
            })}
          </fieldset>
        ) : null,
      )}
      {(['secret_profiles', 'workload_identity_profiles'] as const).map((field) =>
        logicalChoices(field).length ? (
          <fieldset key={field}>
            <legend>
              {t(
                field === 'secret_profiles' ? 'factory.secretProfiles' : 'factory.identityProfiles',
              )}
            </legend>
            {unavailable(supported(CATEGORY[field]))}
            {logicalChoices(field).map(({ value: alias, retained }) => (
              <label key={alias}>
                <input
                  type="checkbox"
                  disabled={!supported(CATEGORY[field]) && !value[field].includes(alias)}
                  checked={value[field].includes(alias)}
                  onChange={(event) =>
                    onChange({
                      ...value,
                      [field]: event.target.checked
                        ? [...value[field], alias]
                        : value[field].filter((item) => item !== alias),
                    })
                  }
                />
                {alias}
                {retainedHint(retained)}
              </label>
            ))}
          </fieldset>
        ) : null,
      )}
      {hosts.length ? (
        <fieldset>
          <legend>{t('factory.networkHosts')}</legend>
          {unavailable(supported('network_hosts'))}
          {hosts.map(({ value: host, retained }) => (
            <label key={host}>
              <input
                type="checkbox"
                disabled={!supported('network_hosts') && !value.network_hosts.includes(host)}
                checked={value.network_hosts.includes(host)}
                onChange={(event) =>
                  onChange({
                    ...value,
                    network_hosts: event.target.checked
                      ? [...value.network_hosts, host]
                      : value.network_hosts.filter((item) => item !== host),
                  })
                }
              />
              {t('factory.networkHost')} · {host}
              {retainedHint(retained)}
            </label>
          ))}
        </fieldset>
      ) : null}
      {mounts.length ? (
        <fieldset>
          <legend>{t('factory.mounts')}</legend>
          {unavailable(supported('filesystem_paths', 'mount_modes'))}
          {mounts.map(({ value: mount, retained }) => (
            <label key={`${mount.root}:${mount.mode}`}>
              <input
                type="checkbox"
                disabled={
                  !supported('filesystem_paths', 'mount_modes') &&
                  !value.mounts.some((item) => item.root === mount.root && item.mode === mount.mode)
                }
                checked={value.mounts.some(
                  (item) => item.root === mount.root && item.mode === mount.mode,
                )}
                onChange={(event) =>
                  onChange({
                    ...value,
                    mounts: event.target.checked
                      ? [...value.mounts, mount]
                      : value.mounts.filter(
                          (item) => item.root !== mount.root || item.mode !== mount.mode,
                        ),
                  })
                }
              />
              {t('factory.mount')} · {mount.root} · {mount.mode}
              {retainedHint(retained)}
            </label>
          ))}
        </fieldset>
      ) : null}
      {commands.length ? (
        <fieldset>
          <legend>{t('factory.commandPermissions')}</legend>
          {unavailable(supported('command_arguments', 'executable_identity'))}
          {commands.map(({ value: command, retained }) => (
            <label key={JSON.stringify(command)}>
              <input
                type="checkbox"
                disabled={
                  !supported('command_arguments', 'executable_identity') &&
                  !value.commands.some((item) => JSON.stringify(item) === JSON.stringify(command))
                }
                checked={value.commands.some(
                  (item) => JSON.stringify(item) === JSON.stringify(command),
                )}
                onChange={(event) =>
                  onChange({
                    ...value,
                    executables:
                      event.target.checked &&
                      !value.executables.some(
                        (reference) =>
                          reference.identity === command.executable.identity &&
                          reference.version === command.executable.version &&
                          reference.digest === command.executable.digest,
                      )
                        ? [...value.executables, command.executable]
                        : value.executables,
                    commands: event.target.checked
                      ? [...value.commands, command]
                      : value.commands.filter(
                          (item) => JSON.stringify(item) !== JSON.stringify(command),
                        ),
                  })
                }
              />
              {t('factory.commandPermission')} · {command.executable.identity} ·{' '}
              {command.executable.version} ·{' '}
              <code>
                {command.arguments
                  .map((argument) =>
                    argument.kind === 'any' ? '*' : JSON.stringify(argument.value),
                  )
                  .join(' ')}
              </code>
              {retainedHint(retained)}
            </label>
          ))}
        </fieldset>
      ) : null}
      {outputKinds.length ? (
        <fieldset>
          <legend>{t('factory.outputKinds')}</legend>
          {unavailable(supported('outputs'))}
          {outputKinds.map(({ value: kind, retained }) => (
            <label key={kind}>
              <input
                type="checkbox"
                disabled={!supported('outputs') && !value.outputs.kinds.includes(kind)}
                checked={value.outputs.kinds.includes(kind)}
                onChange={(event) =>
                  onChange({
                    ...value,
                    outputs: {
                      ...value.outputs,
                      kinds: event.target.checked
                        ? [...value.outputs.kinds, kind]
                        : value.outputs.kinds.filter((item) => item !== kind),
                    },
                  })
                }
              />
              {t('factory.outputKind')} · {kind}
              {retainedHint(retained)}
            </label>
          ))}
        </fieldset>
      ) : null}
      {descendantsEditable ? (
        <label>
          {t('factory.descendants')}
          <input
            type="number"
            min={0}
            max={
              supported('descendant_processes')
                ? (ceiling?.max_descendants ?? Number.MAX_SAFE_INTEGER)
                : 0
            }
            value={value.max_descendants}
            onChange={(event) =>
              onChange({ ...value, max_descendants: Number(event.target.value) })
            }
          />
        </label>
      ) : null}
      {unavailable(resourceFields.every((key) => supported(RESOURCE_CATEGORY[key])))}
      <div className={styles.numericGrid}>
        {resourceFields.map((key) => (
          <label key={key}>
            {t('factory.resource')} · {key}
            <input
              type="number"
              min={supported(RESOURCE_CATEGORY[key]) ? 1 : 0}
              max={
                supported(RESOURCE_CATEGORY[key])
                  ? (ceiling?.resources[key] ?? Number.MAX_SAFE_INTEGER)
                  : 0
              }
              value={value.resources[key]}
              onChange={(event) =>
                onChange({
                  ...value,
                  resources: { ...value.resources, [key]: Number(event.target.value) },
                })
              }
            />
          </label>
        ))}
      </div>
      <div className={styles.numericGrid}>
        {outputFields.map((key) => (
          <label key={key}>
            {t('factory.outputLimit')} · {key}
            <input
              type="number"
              min={0}
              max={
                supported('outputs')
                  ? ceiling
                    ? Number(ceiling.outputs[key])
                    : Number.MAX_SAFE_INTEGER
                  : 0
              }
              value={Number(value.outputs[key])}
              onChange={(event) =>
                onChange({
                  ...value,
                  outputs: { ...value.outputs, [key]: Number(event.target.value) },
                })
              }
            />
          </label>
        ))}
      </div>
    </fieldset>
  );
}
