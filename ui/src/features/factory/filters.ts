import type { components } from '../../../.generated/api/schema';
import type { FactoryFilters } from './api';

export const FACTORY_STATES = {
  admitted: 'Admitted',
  running: 'Running',
  waiting: 'Waiting',
  cancelled: 'Cancelled',
  completed: 'Completed',
} satisfies Record<components['schemas']['FactoryRunState'], string>;
export const FACTORY_DECISIONS = {
  accept: 'Accept',
  rework: 'Rework',
  reject: 'Reject',
  escalate: 'Escalate',
  cancel: 'Cancel',
} satisfies Record<components['schemas']['FactoryDecisionOutcome'], string>;
export const FACTORY_DELIVERY_STATES = {
  succeeded: 'Succeeded',
  failed: 'Failed',
  unknown: 'Unknown',
} satisfies Record<components['schemas']['FactoryDeliveryState'], string>;

/** Only documented, bounded server filters may be copied into requests or links. */
export function readFactoryFilters(params: URLSearchParams): FactoryFilters {
  const result: FactoryFilters = {};
  const one = (key: string) => (params.getAll(key).length === 1 ? params.get(key) : null);
  const state = one('state');
  if (state !== null && Object.hasOwn(FACTORY_STATES, state))
    result.state = state as keyof typeof FACTORY_STATES;
  const decision = one('decision');
  if (decision !== null && Object.hasOwn(FACTORY_DECISIONS, decision))
    result.decision = decision as keyof typeof FACTORY_DECISIONS;
  const delivery = one('delivery_state');
  if (delivery !== null && Object.hasOwn(FACTORY_DELIVERY_STATES, delivery))
    result.delivery_state = delivery as keyof typeof FACTORY_DELIVERY_STATES;
  const configuration = one('configuration_id');
  if (
    configuration !== null &&
    /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u.test(configuration)
  )
    result.configuration_id = configuration;
  const source = one('source');
  if (source !== null && /^[a-zA-Z0-9_.-]{1,128}$/u.test(source)) result.source = source;
  for (const key of ['admitted_from_unix_ms', 'admitted_before_unix_ms'] as const) {
    const raw = one(key);
    if (raw !== null && /^\d+$/u.test(raw)) {
      const value = Number(raw);
      if (Number.isSafeInteger(value)) result[key] = value;
    }
  }
  if (
    result.admitted_from_unix_ms !== undefined &&
    result.admitted_before_unix_ms !== undefined &&
    result.admitted_from_unix_ms >= result.admitted_before_unix_ms
  ) {
    delete result.admitted_from_unix_ms;
    delete result.admitted_before_unix_ms;
  }
  return result;
}

export function factorySearch(filters: FactoryFilters): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filters))
    if (value !== undefined) params.set(key, String(value));
  return params.size === 0 ? '' : `?${params.toString()}`;
}

export function factoryResourceId(raw: string | null): string | null {
  return raw !== null && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/iu.test(raw)
    ? raw.toLowerCase()
    : null;
}
export function readFactoryProjectId(params: URLSearchParams): string | null {
  return params.getAll('project_id').length === 1
    ? factoryResourceId(params.get('project_id'))
    : null;
}
