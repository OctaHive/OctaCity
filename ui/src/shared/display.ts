import type { components } from '../../.generated/api/schema';
import type { MessageKey } from '../app/presentation/messages';

type Schemas = components['schemas'];
type OperatorEnum =
  | Schemas['AgentCurrentLeaseState']
  | Schemas['AgentPoolDefinition']['admission_policy']['mode']
  | Schemas['AgentPoolDefinition']['drain_state']
  | Schemas['AgentPoolDefinition']['fairness_policy']
  | NonNullable<Schemas['AgentRuntimeCapability']['isolation']>
  | Schemas['AgentRuntimeCapability']['mode']
  | Schemas['AgentStatus']
  | Schemas['AttemptState']
  | Schemas['AuditActorKind']
  | Schemas['AuditOutcome']
  | Schemas['BuildResultHoldResource']['state']
  | Schemas['BuildState']
  | Schemas['CacheSessionState']
  | Schemas['DependencyPolicy']
  | Schemas['ExecutionGuarantee']
  | NonNullable<NonNullable<Schemas['JobResource']['terminal']>['failure_classification']>
  | Schemas['JobResource']['placement']['runtime_class']
  | Schemas['JobResource']['state']
  | Schemas['MutationDisposition']
  | Schemas['PoolExecutionTarget']['mode']
  | Schemas['TriggerKind'];

const ENUM_MESSAGE_KEYS = {
  accepted: 'enum.accepted',
  accepting: 'enum.accepting',
  active: 'enum.active',
  adapter: 'enum.adapter',
  agent: 'enum.agent',
  all_completed: 'enum.allCompleted',
  all_succeeded: 'enum.allSucceeded',
  allowlist: 'enum.allowlist',
  any: 'enum.any',
  any_succeeded: 'enum.anySucceeded',
  applied: 'enum.applied',
  authenticated_management: 'enum.authenticatedManagement',
  blocked: 'enum.blocked',
  cancelled: 'enum.cancelled',
  cancelling: 'enum.cancelling',
  cancellation_requested: 'enum.cancellationRequested',
  configuration_fair: 'enum.configurationFair',
  created: 'enum.created',
  dependency_policy: 'enum.dependencyPolicy',
  drained: 'enum.drained',
  draining: 'enum.draining',
  drain_requested: 'enum.drainRequested',
  execution: 'enum.execution',
  execution_allowlist: 'enum.executionAllowlist',
  expired: 'enum.expired',
  external: 'enum.external',
  failed: 'enum.failed',
  fenced: 'enum.fenced',
  filesystem_isolation: 'enum.filesystemIsolation',
  forced_drain: 'enum.forcedDrain',
  graceful_drain: 'enum.gracefulDrain',
  hardware_virtualization: 'enum.hardwareVirtualization',
  host: 'enum.host',
  hypervisor: 'enum.hypervisor',
  infrastructure: 'enum.infrastructure',
  internal: 'enum.internal',
  isolation: 'enum.isolation',
  leased: 'enum.leased',
  manual: 'enum.manual',
  native: 'enum.native',
  network_isolation: 'enum.networkIsolation',
  oci: 'enum.oci',
  oci_hypervisor: 'enum.ociHypervisor',
  oci_process: 'enum.ociProcess',
  offline: 'enum.offline',
  online: 'enum.online',
  orchestrator: 'enum.orchestrator',
  priority_fifo: 'enum.priorityFifo',
  process: 'enum.process',
  process_isolation: 'enum.processIsolation',
  queued: 'enum.queued',
  ready: 'enum.ready',
  released: 'enum.released',
  replayed: 'enum.replayed',
  resource_isolation: 'enum.resourceIsolation',
  revoked: 'enum.revoked',
  running: 'enum.running',
  scheduled: 'enum.scheduled',
  skipped: 'enum.skipped',
  succeeded: 'enum.succeeded',
  trigger: 'enum.trigger',
  unauthenticated_management: 'enum.unauthenticatedManagement',
  virtualization: 'enum.virtualization',
  worker: 'enum.worker',
} as const satisfies Record<OperatorEnum, MessageKey>;

/** Translates every operator-visible API enum through one exhaustive generated-type mapping. */
export function formatEnumLabel(value: OperatorEnum, t: (key: MessageKey) => string): string {
  return t(ENUM_MESSAGE_KEYS[value]);
}

/** Formats an epoch timestamp for an explicit locale and keeps a stable machine value. */
export function formatTimestamp(
  value: number,
  locale: string,
): { display: string; machine: string | null } {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return { display: locale.startsWith('ru') ? 'Неизвестно' : 'Unknown', machine: null };
  }
  const machine = date.toISOString();
  const display = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'medium',
    timeZone: 'UTC',
  }).format(date);
  return { display: `${display} UTC`, machine };
}

/** Formats an exact byte count for an explicit locale in compact diagnostic tables. */
export function formatBytes(value: number, locale: string): string {
  const [amount, unit] =
    value < 1_024
      ? [value, 'B']
      : value < 1_048_576
        ? [value / 1_024, 'KiB']
        : value < 1_073_741_824
          ? [value / 1_048_576, 'MiB']
          : [value / 1_073_741_824, 'GiB'];
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: unit === 'B' ? 0 : 1,
    minimumFractionDigits: unit === 'B' ? 0 : 1,
  }).format(amount);
  return `${formatted} ${unit}`;
}
