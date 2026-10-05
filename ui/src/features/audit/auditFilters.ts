import {
  AUDIT_ACTOR_KINDS as GENERATED_AUDIT_ACTOR_KINDS,
  AUDIT_QUERY_CONSTRAINTS,
} from '../../../.generated/api/constraints';
import type { MessageKey, MessageValues } from '../../app/presentation/messages';
import type { AuditActorKind, AuditFilters } from './api';

export const AUDIT_ACTOR_KINDS = GENERATED_AUDIT_ACTOR_KINDS satisfies readonly AuditActorKind[];

export const AUDIT_TEXT_FILTERS = [
  {
    label: 'audit.actorIdentity',
    maximumBytes: AUDIT_QUERY_CONSTRAINTS.actorIdentityMaximumBytes,
    name: 'actor_identity',
  },
  {
    label: 'audit.operation',
    maximumBytes: AUDIT_QUERY_CONSTRAINTS.operationMaximumBytes,
    name: 'operation',
  },
  {
    label: 'audit.targetKind',
    maximumBytes: AUDIT_QUERY_CONSTRAINTS.targetKindMaximumBytes,
    name: 'target_kind',
  },
  {
    label: 'audit.targetIdentity',
    maximumBytes: AUDIT_QUERY_CONSTRAINTS.targetIdentityMaximumBytes,
    name: 'target_identity',
  },
  {
    label: 'audit.requestIdentity',
    maximumBytes: AUDIT_QUERY_CONSTRAINTS.requestIdentityMaximumBytes,
    name: 'request_identity',
  },
] as const;

type Translate = (key: MessageKey, values?: MessageValues) => string;

const FILTER_PARAMETERS = [
  'actor_kind',
  ...AUDIT_TEXT_FILTERS.map(({ name }) => name),
  'occurred_from_unix_ms',
  'occurred_through_unix_ms',
] as const;

type TextFilterName = (typeof AUDIT_TEXT_FILTERS)[number]['name'];

export interface ParsedAuditFilters {
  error: string | null;
  filters: AuditFilters;
}

/** Decodes and bounds every shareable audit filter before a server query is enabled. */
export function readAuditFilters(parameters: URLSearchParams, t: Translate): ParsedAuditFilters {
  const actorKindValue = parameters.get('actor_kind');
  const actorKind =
    actorKindValue === null || actorKindValue === ''
      ? null
      : AUDIT_ACTOR_KINDS.find((kind) => kind === actorKindValue);
  if (actorKind === undefined) return invalidFilters(t('audit.actorKindUnsupported'));

  const text = readTextFilters(parameters, t);
  if (text.error !== null) return invalidFilters(text.error);
  const from = readUnixTime(parameters.get('occurred_from_unix_ms'), t('audit.occurredFrom'), t);
  if (from.error !== null) return invalidFilters(from.error);
  const through = readUnixTime(
    parameters.get('occurred_through_unix_ms'),
    t('audit.occurredThrough'),
    t,
  );
  if (through.error !== null) return invalidFilters(through.error);
  if (from.value !== null && through.value !== null && from.value > through.value) {
    return invalidFilters(t('audit.occurredRangeInvalid'));
  }

  return {
    error: null,
    filters: {
      actorIdentity: text.values.actor_identity,
      actorKind,
      occurredFromUnixMs: from.value,
      occurredThroughUnixMs: through.value,
      operation: text.values.operation,
      requestIdentity: text.values.request_identity,
      targetIdentity: text.values.target_identity,
      targetKind: text.values.target_kind,
    },
  };
}

/** Converts the filter form into the same validated representation used for copied URLs. */
export function auditFiltersFromForm(data: FormData, t: Translate): ParsedAuditFilters {
  const parameters = new URLSearchParams();
  const actorKind = String(data.get('actor_kind') ?? '');
  if (actorKind !== '') parameters.set('actor_kind', actorKind);
  for (const { name } of AUDIT_TEXT_FILTERS) {
    const value = String(data.get(name) ?? '');
    if (value !== '') parameters.set(name, value);
  }
  const from = dateInputToUnixTime(
    String(data.get('occurred_from') ?? ''),
    t('audit.occurredFrom'),
    t,
  );
  if (from.error !== null) return invalidFilters(from.error);
  const through = dateInputToUnixTime(
    String(data.get('occurred_through') ?? ''),
    t('audit.occurredThrough'),
    t,
  );
  if (through.error !== null) return invalidFilters(through.error);
  if (from.value !== null) parameters.set('occurred_from_unix_ms', String(from.value));
  if (through.value !== null) parameters.set('occurred_through_unix_ms', String(through.value));
  return readAuditFilters(parameters, t);
}

/** Replaces only audit-owned URL parameters with one canonical bounded filter set. */
export function encodeAuditFilters(
  current: URLSearchParams,
  filters: AuditFilters,
): URLSearchParams {
  const next = withoutAuditFilters(current);
  const values: Array<[string, string | number | null]> = [
    ['actor_kind', filters.actorKind],
    ['actor_identity', filters.actorIdentity],
    ['operation', filters.operation],
    ['target_kind', filters.targetKind],
    ['target_identity', filters.targetIdentity],
    ['request_identity', filters.requestIdentity],
    ['occurred_from_unix_ms', filters.occurredFromUnixMs],
    ['occurred_through_unix_ms', filters.occurredThroughUnixMs],
  ];
  for (const [name, value] of values) {
    if (value !== null) next.set(name, String(value));
  }
  return next;
}

export function withoutAuditFilters(parameters: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(parameters);
  for (const name of FILTER_PARAMETERS) next.delete(name);
  return next;
}

export function hasAuditFilters(parameters: URLSearchParams): boolean {
  return FILTER_PARAMETERS.some((name) => parameters.has(name));
}

/** Converts a validated Unix-millisecond filter to a lossless local datetime control value. */
export function auditTimeInputValue(value: string | null): string {
  if (value === null || !/^-?\d+$/u.test(value)) return '';
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || Number.isNaN(new Date(parsed).getTime())) return '';
  const date = new Date(parsed);
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${String(date.getMilliseconds()).padStart(3, '0')}`;
}

function readTextFilters(
  parameters: URLSearchParams,
  t: Translate,
): {
  error: string | null;
  values: Record<TextFilterName, string | null>;
} {
  const values = {} as Record<TextFilterName, string | null>;
  for (const { label, maximumBytes, name } of AUDIT_TEXT_FILTERS) {
    const value = parameters.get(name);
    if (value === null || value === '') {
      values[name] = null;
      continue;
    }
    if (
      value.trim() !== value ||
      hasControlCharacter(value) ||
      new TextEncoder().encode(value).byteLength > maximumBytes
    ) {
      return {
        error: t('audit.filterInvalid', { label: t(label), maximum: maximumBytes }),
        values,
      };
    }
    values[name] = value;
  }
  return { error: null, values };
}

function hasControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return codePoint !== undefined && (codePoint <= 31 || codePoint === 127);
  });
}

function readUnixTime(
  value: string | null,
  label: string,
  t: Translate,
): { error: string | null; value: number | null } {
  if (value === null || value === '') return { error: null, value: null };
  if (!/^-?\d+$/u.test(value)) return { error: t('audit.timeUnixInvalid', { label }), value: null };
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || Number.isNaN(new Date(parsed).getTime())) {
    return { error: t('audit.timeUnixUnrepresentable', { label }), value: null };
  }
  return { error: null, value: parsed };
}

function invalidFilters(error: string): ParsedAuditFilters {
  return {
    error,
    filters: {
      actorIdentity: null,
      actorKind: null,
      occurredFromUnixMs: null,
      occurredThroughUnixMs: null,
      operation: null,
      requestIdentity: null,
      targetIdentity: null,
      targetKind: null,
    },
  };
}

function dateInputToUnixTime(
  value: string,
  label: string,
  t: Translate,
): { error: string | null; value: number | null } {
  if (value === '') return { error: null, value: null };
  const parsed = Date.parse(value);
  return Number.isNaN(parsed)
    ? { error: t('audit.timeLocalInvalid', { label }), value: null }
    : { error: null, value: parsed };
}
