import { describe, expect, it } from 'vitest';

import {
  EXPLORER_DEFAULT_WIDTH,
  EXPLORER_MAX_WIDTH,
  EXPLORER_MIN_WIDTH,
  clampExplorerWidth,
  loadExplorerWidth,
  saveExplorerWidth,
} from './preferences';

describe('explorer preferences', () => {
  it('clamps finite widths and resets non-finite values', () => {
    expect(clampExplorerWidth(EXPLORER_MIN_WIDTH - 100)).toBe(EXPLORER_MIN_WIDTH);
    expect(clampExplorerWidth(317.6)).toBe(318);
    expect(clampExplorerWidth(EXPLORER_MAX_WIDTH + 100)).toBe(EXPLORER_MAX_WIDTH);
    expect(clampExplorerWidth(Number.NaN)).toBe(EXPLORER_DEFAULT_WIDTH);
  });

  it.each(['not-json', '{}', '{"version":2,"explorerWidth":320}', '{"version":1}'])(
    'ignores an invalid record: %s',
    (record) => {
      expect(loadExplorerWidth({ getItem: () => record })).toBe(EXPLORER_DEFAULT_WIDTH);
    },
  );

  it('loads and persists only a versioned clamped width', () => {
    let record: string | null = '{"version":1,"explorerWidth":999}';
    const storage = {
      getItem: () => record,
      setItem: (_key: string, value: string) => {
        record = value;
      },
    };

    expect(loadExplorerWidth(storage)).toBe(EXPLORER_MAX_WIDTH);
    expect(saveExplorerWidth(200, storage)).toBe(EXPLORER_MIN_WIDTH);
    expect(record).toBe(`{"explorerWidth":${EXPLORER_MIN_WIDTH},"version":1}`);
  });

  it('remains usable when browser storage rejects reads or writes', () => {
    expect(
      loadExplorerWidth({
        getItem: () => {
          throw new Error('blocked');
        },
      }),
    ).toBe(EXPLORER_DEFAULT_WIDTH);
    expect(
      saveExplorerWidth(312, {
        setItem: () => {
          throw new Error('blocked');
        },
      }),
    ).toBe(312);
  });
});
