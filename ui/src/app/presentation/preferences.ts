export const EXPLORER_DEFAULT_WIDTH = 288;
export const EXPLORER_MAX_WIDTH = 480;
export const EXPLORER_MIN_WIDTH = 240;
export const EXPLORER_RESIZE_STEP = 16;

export const PREFERENCES_KEY = 'octacity.console.preferences';
export const PREFERENCES_VERSION = 2;
export const MAX_PREFERENCE_BYTES = 16_384;
export const MAX_EXPANDED_IDENTITIES = 100;
export const MAX_RECENT_IDENTITIES = 20;
export const MAX_FAVORITE_IDENTITIES = 50;

export type Language = 'en' | 'ru';
export type ThemeMode = 'system' | 'light' | 'dark';
export type PreferenceResourceKind =
  'project' | 'build_configuration' | 'build' | 'agent' | 'agent_pool';

export interface PreferenceIdentity {
  readonly id: string;
  readonly kind: PreferenceResourceKind;
}

export interface ConsolePreferences {
  readonly expanded: readonly PreferenceIdentity[];
  readonly explorerOpen: boolean;
  readonly explorerWidth: number;
  readonly favorites: readonly PreferenceIdentity[];
  readonly language: Language;
  readonly notificationLastOpenedAt: number | null;
  readonly recents: readonly PreferenceIdentity[];
  readonly theme: ThemeMode;
  readonly version: typeof PREFERENCES_VERSION;
}

export const DEFAULT_PREFERENCES: ConsolePreferences = {
  expanded: [],
  explorerOpen: true,
  explorerWidth: EXPLORER_DEFAULT_WIDTH,
  favorites: [],
  language: 'en',
  notificationLastOpenedAt: null,
  recents: [],
  theme: 'system',
  version: PREFERENCES_VERSION,
};

export type PreferenceStorage = Pick<Storage, 'getItem' | 'setItem'>;

const UUID_PATTERN = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u;
const NIL_UUID = '00000000-0000-0000-0000-000000000000';
const RESOURCE_KINDS: readonly PreferenceResourceKind[] = [
  'project',
  'build_configuration',
  'build',
  'agent',
  'agent_pool',
];
const RECORD_KEYS = [
  'expanded',
  'explorerOpen',
  'explorerWidth',
  'favorites',
  'language',
  'notificationLastOpenedAt',
  'recents',
  'theme',
  'version',
] as const;

/** Keeps the explorer usable even when a stored or pointer-derived width is invalid. */
export function clampExplorerWidth(width: number): number {
  if (!Number.isFinite(width)) return EXPLORER_DEFAULT_WIDTH;
  return Math.min(EXPLORER_MAX_WIDTH, Math.max(EXPLORER_MIN_WIDTH, Math.round(width)));
}

/** Reads, validates, bounds, and when needed migrates the console's only browser-local record. */
export function loadPreferences(
  storage: PreferenceStorage | undefined = browserStorage(),
  now = Date.now(),
): ConsolePreferences {
  if (storage === undefined) return DEFAULT_PREFERENCES;
  try {
    const raw = storage.getItem(PREFERENCES_KEY);
    if (raw === null) return DEFAULT_PREFERENCES;
    if (new TextEncoder().encode(raw).length > MAX_PREFERENCE_BYTES) {
      return resetPreferences(storage);
    }
    const value: unknown = JSON.parse(raw);
    const migrated = migrateVersionOne(value);
    if (migrated !== null) {
      persist(storage, migrated);
      return migrated;
    }
    const normalized = normalizePreferences(value, now);
    if (normalized === null) return resetPreferences(storage);
    if (JSON.stringify(normalized) !== raw) persist(storage, normalized);
    return normalized;
  } catch {
    return resetPreferences(storage);
  }
}

/** Persists only the normalized, bounded preference schema. */
export function savePreferences(
  preferences: ConsolePreferences,
  storage: PreferenceStorage | undefined = browserStorage(),
  now = Date.now(),
): ConsolePreferences {
  const normalized = normalizePreferences(preferences, now) ?? DEFAULT_PREFERENCES;
  if (storage !== undefined) persist(storage, normalized);
  return normalized;
}

function normalizePreferences(value: unknown, now: number): ConsolePreferences | null {
  if (!isPlainObject(value) || !hasExactKeys(value, RECORD_KEYS) || value.version !== 2) {
    return null;
  }
  return {
    expanded: normalizeIdentities(value.expanded, MAX_EXPANDED_IDENTITIES),
    explorerOpen:
      typeof value.explorerOpen === 'boolean'
        ? value.explorerOpen
        : DEFAULT_PREFERENCES.explorerOpen,
    explorerWidth:
      typeof value.explorerWidth === 'number'
        ? clampExplorerWidth(value.explorerWidth)
        : EXPLORER_DEFAULT_WIDTH,
    favorites: normalizeIdentities(value.favorites, MAX_FAVORITE_IDENTITIES),
    language: value.language === 'ru' ? 'ru' : 'en',
    notificationLastOpenedAt: validTimestamp(value.notificationLastOpenedAt, now),
    recents: normalizeIdentities(value.recents, MAX_RECENT_IDENTITIES),
    theme: value.theme === 'dark' || value.theme === 'light' ? value.theme : 'system',
    version: PREFERENCES_VERSION,
  };
}

function migrateVersionOne(value: unknown): ConsolePreferences | null {
  if (
    !isPlainObject(value) ||
    value.version !== 1 ||
    !hasExactKeys(value, ['explorerWidth', 'version'])
  ) {
    return null;
  }
  return {
    ...DEFAULT_PREFERENCES,
    explorerWidth:
      typeof value.explorerWidth === 'number'
        ? clampExplorerWidth(value.explorerWidth)
        : EXPLORER_DEFAULT_WIDTH,
  };
}

function normalizeIdentities(value: unknown, limit: number): readonly PreferenceIdentity[] {
  if (!Array.isArray(value)) return [];
  const result: PreferenceIdentity[] = [];
  const seen = new Set<string>();
  for (const candidate of value) {
    if (result.length >= limit) break;
    if (!isPreferenceIdentity(candidate)) continue;
    const key = `${candidate.kind}:${candidate.id}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push({ id: candidate.id, kind: candidate.kind });
  }
  return result;
}

function isPreferenceIdentity(value: unknown): value is PreferenceIdentity {
  return (
    isPlainObject(value) &&
    hasExactKeys(value, ['id', 'kind']) &&
    typeof value.id === 'string' &&
    value.id !== NIL_UUID &&
    UUID_PATTERN.test(value.id) &&
    typeof value.kind === 'string' &&
    RESOURCE_KINDS.includes(value.kind as PreferenceResourceKind)
  );
}

function validTimestamp(value: unknown, now: number): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= now
    ? value
    : null;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === allowed.length && keys.every((key) => allowed.includes(key));
}

function resetPreferences(storage: PreferenceStorage): ConsolePreferences {
  persist(storage, DEFAULT_PREFERENCES);
  return DEFAULT_PREFERENCES;
}

function persist(storage: Pick<Storage, 'setItem'>, preferences: ConsolePreferences): void {
  try {
    const serialized = JSON.stringify(preferences);
    storage.setItem(
      PREFERENCES_KEY,
      new TextEncoder().encode(serialized).length <= MAX_PREFERENCE_BYTES
        ? serialized
        : JSON.stringify(DEFAULT_PREFERENCES),
    );
  } catch {
    // Storage can be unavailable in private or policy-restricted browser contexts.
  }
}

function browserStorage(): Storage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}
