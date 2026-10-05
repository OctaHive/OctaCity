// @vitest-environment jsdom

import { describe, expect, it } from 'vitest';

import { translate } from '../app/presentation/messages';
import { formatBytes, formatEnumLabel, formatTimestamp } from './display';

describe('locale-aware display formatting', () => {
  it('formats numbers and stable enum values for the selected language', () => {
    expect(formatBytes(33_554_432, 'en-US')).toBe('32.0 MiB');
    expect(formatBytes(33_554_432, 'ru-RU')).toBe('32,0 MiB');
    expect(formatEnumLabel('cancellation_requested', (key) => translate('ru', key))).toBe(
      'Запрошена отмена',
    );
  });

  it('requires the selected locale and preserves machine timestamps', () => {
    const timestamp = formatTimestamp(Date.UTC(2026, 9, 5, 9, 30), 'ru-RU');

    expect(timestamp.machine).toBe('2026-10-05T09:30:00.000Z');
    expect(timestamp.display).toContain('окт.');
    expect(timestamp.display).toContain('UTC');
  });

  it('localizes invalid timestamps without emitting a machine value', () => {
    expect(formatTimestamp(Number.NaN, 'en-US')).toEqual({ display: 'Unknown', machine: null });
    expect(formatTimestamp(Number.NaN, 'ru-RU')).toEqual({ display: 'Неизвестно', machine: null });
  });
});
