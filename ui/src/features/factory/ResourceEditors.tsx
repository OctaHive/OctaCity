import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import styles from './Factory.module.css';

type Budget = components['schemas']['FactoryBudget'];
type Permissions = components['schemas']['FactoryPermissions'];
type Choice = components['schemas']['FactoryConfigurationChoiceResource'];
type ReferenceField = 'plugins' | 'executables' | 'tools';
type LogicalField = 'secret_profiles' | 'workload_identity_profiles';
type PermissionChoices = readonly Choice[] | Pick<Permissions, ReferenceField | LogicalField>;

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
}: {
  value: Permissions;
  ceiling?: Permissions;
  choices: PermissionChoices;
  onChange: (value: Permissions) => void;
  label: string;
}) {
  const { t } = usePresentation();
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
  const referenceChoices = (field: ReferenceField) =>
    'plugins' in choices
      ? choices[field].map((reference) => ({ alias: reference.identity, reference }))
      : choices.flatMap((choice) =>
          choice.kind === 'node_capability' && choice.reference.kind === 'immutable'
            ? [{ alias: choice.alias, reference: choice.reference.reference }]
            : [],
        );
  const logicalChoices = (field: LogicalField) =>
    'plugins' in choices
      ? choices[field]
      : choices
          .filter(
            (choice) =>
              choice.kind ===
              (field === 'secret_profiles' ? 'secret_profile' : 'workload_identity_profile'),
          )
          .map((choice) => choice.alias);
  return (
    <fieldset>
      <legend>{label}</legend>
      {(['plugins', 'executables', 'tools'] as const).map((field) => (
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
          {referenceChoices(field).map(({ alias, reference }) => {
            return (
              <label
                key={`${alias}:${reference.identity}:${reference.version}:${reference.digest}`}
              >
                <input
                  type="checkbox"
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
              </label>
            );
          })}
        </fieldset>
      ))}
      {(['secret_profiles', 'workload_identity_profiles'] as const).map((field) => (
        <fieldset key={field}>
          <legend>
            {t(field === 'secret_profiles' ? 'factory.secretProfiles' : 'factory.identityProfiles')}
          </legend>
          {logicalChoices(field).map((alias) => (
            <label key={alias}>
              <input
                type="checkbox"
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
            </label>
          ))}
        </fieldset>
      ))}
      {ceiling ? (
        <>
          <fieldset>
            <legend>{t('factory.networkHosts')}</legend>
            {ceiling.network_hosts.map((host) => (
              <label key={host}>
                <input
                  type="checkbox"
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
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>{t('factory.mounts')}</legend>
            {ceiling.mounts.map((mount) => (
              <label key={`${mount.root}:${mount.mode}`}>
                <input
                  type="checkbox"
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
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>{t('factory.commandPermissions')}</legend>
            {ceiling.commands.map((command) => (
              <label key={JSON.stringify(command)}>
                <input
                  type="checkbox"
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
                {command.executable.version}
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>{t('factory.outputKinds')}</legend>
            {ceiling.outputs.kinds.map((kind) => (
              <label key={kind}>
                <input
                  type="checkbox"
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
              </label>
            ))}
          </fieldset>
        </>
      ) : null}
      <label>
        {t('factory.descendants')}
        <input
          type="number"
          min={0}
          max={Number.MAX_SAFE_INTEGER}
          value={value.max_descendants}
          onChange={(event) => onChange({ ...value, max_descendants: Number(event.target.value) })}
        />
      </label>
      <div className={styles.numericGrid}>
        {(Object.keys(value.resources) as (keyof Permissions['resources'])[]).map((key) => (
          <label key={key}>
            {t('factory.resource')} · {key}
            <input
              type="number"
              min={1}
              max={Number.MAX_SAFE_INTEGER}
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
        {(Object.keys(value.outputs) as (keyof Permissions['outputs'])[])
          .filter((key) => key !== 'kinds')
          .map((key) => (
            <label key={key}>
              {t('factory.outputLimit')} · {key}
              <input
                type="number"
                min={0}
                max={Number.MAX_SAFE_INTEGER}
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
