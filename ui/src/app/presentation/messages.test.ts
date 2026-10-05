import { describe, expect, it } from 'vitest';

import { catalogs, translate } from './messages';

describe('operator message catalogs', () => {
  it('keeps English and Russian keys identical', () => {
    expect(Object.keys(catalogs.ru).sort()).toEqual(Object.keys(catalogs.en).sort());
  });

  it('keeps non-empty placeholder contracts identical in both languages', () => {
    for (const key of Object.keys(catalogs.en) as Array<keyof typeof catalogs.en>) {
      expect(catalogs.en[key].trim(), key).not.toBe('');
      expect(catalogs.ru[key].trim(), key).not.toBe('');
      expect(placeholders(catalogs.ru[key]), key).toEqual(placeholders(catalogs.en[key]));
    }
  });

  it('formats typed placeholders without evaluating catalog content', () => {
    expect(translate('ru', 'shell.searchSection', { section: 'Сборки' })).toBe(
      'Искать в разделе «Сборки»',
    );
  });
});

function placeholders(message: string): string[] {
  return [...message.matchAll(/\{(?<name>[a-zA-Z][a-zA-Z0-9]*)\}/gu)]
    .map((match) => match.groups?.name ?? '')
    .sort();
}
