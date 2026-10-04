export const EXPLORER_DEFAULT_WIDTH = 288;
export const EXPLORER_MAX_WIDTH = 480;
export const EXPLORER_MIN_WIDTH = 240;
export const EXPLORER_RESIZE_STEP = 16;

const PREFERENCES_KEY = 'octacity.console.preferences';
const PREFERENCES_VERSION = 1;

interface StoredPreferences {
  explorerWidth: number;
  version: typeof PREFERENCES_VERSION;
}

type PreferenceReader = Pick<Storage, 'getItem'>;
type PreferenceWriter = Pick<Storage, 'setItem'>;

/** Keeps the explorer usable even when a stored or pointer-derived width is invalid. */
export function clampExplorerWidth(width: number): number {
  if (!Number.isFinite(width)) {
    return EXPLORER_DEFAULT_WIDTH;
  }
  return Math.min(EXPLORER_MAX_WIDTH, Math.max(EXPLORER_MIN_WIDTH, Math.round(width)));
}

/** Loads only the versioned explorer-width presentation preference. */
export function loadExplorerWidth(
  storage: PreferenceReader | undefined = browserStorage(),
): number {
  if (storage === undefined) {
    return EXPLORER_DEFAULT_WIDTH;
  }
  try {
    const value: unknown = JSON.parse(storage.getItem(PREFERENCES_KEY) ?? 'null');
    if (!isStoredPreferences(value)) {
      return EXPLORER_DEFAULT_WIDTH;
    }
    return clampExplorerWidth(value.explorerWidth);
  } catch {
    return EXPLORER_DEFAULT_WIDTH;
  }
}

/** Persists a clamped width without exposing a general browser-storage boundary. */
export function saveExplorerWidth(
  width: number,
  storage: PreferenceWriter | undefined = browserStorage(),
): number {
  const explorerWidth = clampExplorerWidth(width);
  if (storage === undefined) {
    return explorerWidth;
  }
  try {
    const value: StoredPreferences = { explorerWidth, version: PREFERENCES_VERSION };
    storage.setItem(PREFERENCES_KEY, JSON.stringify(value));
  } catch {
    // Storage can be unavailable in private or policy-restricted browser contexts.
  }
  return explorerWidth;
}

function browserStorage(): Storage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

function isStoredPreferences(value: unknown): value is StoredPreferences {
  return (
    typeof value === 'object' &&
    value !== null &&
    'version' in value &&
    value.version === PREFERENCES_VERSION &&
    'explorerWidth' in value &&
    typeof value.explorerWidth === 'number'
  );
}
