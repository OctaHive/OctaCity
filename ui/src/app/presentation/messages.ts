import { enMessages, type MessageCatalog, type MessageKey } from './messages.en';
import { ruMessages } from './messages.ru';
import type { Language } from './preferences';

export type MessageValues = Readonly<Record<string, number | string>>;

const catalogs = { en: enMessages, ru: ruMessages } satisfies Record<Language, MessageCatalog>;

export function translate(language: Language, key: MessageKey, values: MessageValues = {}): string {
  return catalogs[language][key].replaceAll(/\{(?<name>[a-zA-Z][a-zA-Z0-9]*)\}/gu, (token, name) =>
    Object.hasOwn(values, name) ? String(values[name]) : token,
  );
}

export { catalogs, type MessageKey };
