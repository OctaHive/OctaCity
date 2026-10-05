import { describe, expect, it } from 'vitest';

import {
  DEFAULT_PREFERENCES,
  EXPLORER_DEFAULT_WIDTH,
  EXPLORER_MAX_WIDTH,
  EXPLORER_MIN_WIDTH,
  MAX_EXPANDED_IDENTITIES,
  MAX_FAVORITE_IDENTITIES,
  MAX_PREFERENCE_BYTES,
  MAX_RECENT_IDENTITIES,
  PREFERENCES_KEY,
  clampExplorerWidth,
  loadPreferences,
  savePreferences,
  type ConsolePreferences,
} from './preferences';

class MemoryStorage {
  record: string | null;

  constructor(record: string | null = null) {
    this.record = record;
  }

  getItem(key: string) {
    expect(key).toBe(PREFERENCES_KEY);
    return this.record;
  }

  setItem(key: string, value: string) {
    expect(key).toBe(PREFERENCES_KEY);
    this.record = value;
  }
}

describe('console preferences', () => {
  it('clamps finite widths and resets non-finite values', () => {
    expect(clampExplorerWidth(EXPLORER_MIN_WIDTH - 100)).toBe(EXPLORER_MIN_WIDTH);
    expect(clampExplorerWidth(317.6)).toBe(318);
    expect(clampExplorerWidth(EXPLORER_MAX_WIDTH + 100)).toBe(EXPLORER_MAX_WIDTH);
    expect(clampExplorerWidth(Number.NaN)).toBe(EXPLORER_DEFAULT_WIDTH);
  });

  it('migrates the bounded version-one width without inventing other state', () => {
    const storage = new MemoryStorage('{"explorerWidth":999,"version":1}');

    expect(loadPreferences(storage)).toEqual({
      ...DEFAULT_PREFERENCES,
      explorerWidth: EXPLORER_MAX_WIDTH,
    });
    expect(JSON.parse(storage.record ?? '')).toEqual({
      ...DEFAULT_PREFERENCES,
      explorerWidth: EXPLORER_MAX_WIDTH,
    });
  });

  it.each(['not-json', '{}', '{"version":2}', '{"version":99}', '{"version":2,"apiPayload":{}}'])(
    'resets an invalid or non-allowlisted record: %s',
    (record) => {
      const storage = new MemoryStorage(record);
      expect(loadPreferences(storage)).toEqual(DEFAULT_PREFERENCES);
      expect(JSON.parse(storage.record ?? '')).toEqual(DEFAULT_PREFERENCES);
    },
  );

  it('restores valid presentation state and filters unsafe identity entries', () => {
    const validId = '11111111-1111-4111-8111-111111111111';
    const storage = new MemoryStorage(
      JSON.stringify({
        ...DEFAULT_PREFERENCES,
        expanded: [
          { id: validId, kind: 'project' },
          { id: 'not-an-id', kind: 'project' },
          { id: validId, kind: 'project', label: 'server-derived' },
        ],
        explorerOpen: false,
        explorerWidth: 1000,
        favorites: [{ id: validId, kind: 'build' }],
        language: 'ru',
        notificationLastOpenedAt: 1_700_000_000_000,
        recents: [{ id: validId, kind: 'agent' }],
        theme: 'dark',
      }),
    );

    expect(loadPreferences(storage, 1_800_000_000_000)).toEqual({
      expanded: [{ id: validId, kind: 'project' }],
      explorerOpen: false,
      explorerWidth: EXPLORER_MAX_WIDTH,
      favorites: [{ id: validId, kind: 'build' }],
      language: 'ru',
      notificationLastOpenedAt: 1_700_000_000_000,
      recents: [{ id: validId, kind: 'agent' }],
      theme: 'dark',
      version: 2,
    });
  });

  it('bounds each identity collection and removes duplicates', () => {
    const identities = Array.from({ length: MAX_EXPANDED_IDENTITIES + 5 }, (_, index) => ({
      id: `00000000-0000-4000-8000-${String(index).padStart(12, '0')}`,
      kind: 'project' as const,
    }));
    const stored: ConsolePreferences = {
      ...DEFAULT_PREFERENCES,
      expanded: [...identities, identities[0]!],
      favorites: identities.slice(0, MAX_FAVORITE_IDENTITIES + 5),
      recents: identities.slice(0, MAX_RECENT_IDENTITIES + 5),
    };
    const storage = new MemoryStorage();

    savePreferences(stored, storage);
    const loaded = loadPreferences(storage);
    expect(loaded.expanded).toHaveLength(MAX_EXPANDED_IDENTITIES);
    expect(loaded.favorites).toHaveLength(MAX_FAVORITE_IDENTITIES);
    expect(loaded.recents).toHaveLength(MAX_RECENT_IDENTITIES);
    expect(new Set(loaded.expanded.map(({ id }) => id)).size).toBe(loaded.expanded.length);
  });

  it('recovers from oversized records without parsing or retaining them', () => {
    const storage = new MemoryStorage('x'.repeat(MAX_PREFERENCE_BYTES + 1));
    expect(loadPreferences(storage)).toEqual(DEFAULT_PREFERENCES);
    expect(JSON.parse(storage.record ?? '')).toEqual(DEFAULT_PREFERENCES);
  });

  it('rejects future and malformed locally generated timestamps', () => {
    for (const value of [-1, Number.NaN, 2_000]) {
      const storage = new MemoryStorage(
        JSON.stringify({ ...DEFAULT_PREFERENCES, notificationLastOpenedAt: value }),
      );
      expect(loadPreferences(storage, 1_000).notificationLastOpenedAt).toBeNull();
    }
  });

  it('remains usable when browser storage rejects reads or writes', () => {
    expect(
      loadPreferences({
        getItem: () => {
          throw new Error('blocked');
        },
        setItem: () => undefined,
      }),
    ).toEqual(DEFAULT_PREFERENCES);
    expect(() =>
      savePreferences(DEFAULT_PREFERENCES, {
        getItem: () => null,
        setItem: () => {
          throw new Error('blocked');
        },
      }),
    ).not.toThrow();
  });

  it('serializes only the declared presentation allowlist', () => {
    const storage = new MemoryStorage();
    savePreferences(DEFAULT_PREFERENCES, storage);

    expect(Object.keys(JSON.parse(storage.record ?? '')).sort()).toEqual(
      [
        'expanded',
        'explorerOpen',
        'explorerWidth',
        'favorites',
        'language',
        'notificationLastOpenedAt',
        'recents',
        'theme',
        'version',
      ].sort(),
    );
    expect(storage.record).not.toMatch(
      /api|payload|notificationItems|query|mutation|log|requestId|credential|download/i,
    );
  });
});
