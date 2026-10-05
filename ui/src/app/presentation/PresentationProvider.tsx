import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import {
  DEFAULT_PREFERENCES,
  MAX_EXPANDED_IDENTITIES,
  MAX_FAVORITE_IDENTITIES,
  MAX_RECENT_IDENTITIES,
  loadPreferences,
  savePreferences,
  type ConsolePreferences,
  type Language,
  type PreferenceIdentity,
  type PreferenceStorage,
  type ThemeMode,
} from './preferences';
import { translate, type MessageKey, type MessageValues } from './messages';

export type ResolvedTheme = 'light' | 'dark';

interface PresentationContextValue {
  addRecent: (identity: PreferenceIdentity) => void;
  locale: string;
  markNotificationsOpened: () => void;
  preferences: ConsolePreferences;
  resolvedTheme: ResolvedTheme;
  setExpanded: (identity: PreferenceIdentity, expanded: boolean) => void;
  setExplorerOpen: (open: boolean) => void;
  setExplorerWidth: (width: number) => void;
  setLanguage: (language: Language) => void;
  setTheme: (theme: ThemeMode) => void;
  t: (key: MessageKey, values?: MessageValues) => string;
  toggleFavorite: (identity: PreferenceIdentity) => void;
}

const noOperation = () => undefined;
const defaultContext: PresentationContextValue = {
  addRecent: noOperation,
  locale: 'en-US',
  markNotificationsOpened: noOperation,
  preferences: DEFAULT_PREFERENCES,
  resolvedTheme: 'light',
  setExpanded: noOperation,
  setExplorerOpen: noOperation,
  setExplorerWidth: noOperation,
  setLanguage: noOperation,
  setTheme: noOperation,
  t: (key, values) => translate('en', key, values),
  toggleFavorite: noOperation,
};

const PresentationContext = createContext<PresentationContextValue>(defaultContext);

export function PresentationProvider({
  children,
  storage,
}: {
  children: ReactNode;
  storage?: PreferenceStorage | undefined;
}) {
  const [preferences, setPreferences] = useState(() => loadPreferences(storage));
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(() => preferredSystemTheme());
  const resolvedTheme = preferences.theme === 'system' ? systemTheme : preferences.theme;
  const locale = preferences.language === 'ru' ? 'ru-RU' : 'en-US';

  const update = useCallback(
    (transform: (current: ConsolePreferences) => ConsolePreferences) => {
      setPreferences((current) => savePreferences(transform(current), storage));
    },
    [storage],
  );

  useEffect(() => {
    const media = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
    const updateSystemTheme = () => setSystemTheme(media?.matches === true ? 'dark' : 'light');
    media?.addEventListener('change', updateSystemTheme);
    return () => media?.removeEventListener('change', updateSystemTheme);
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.lang = preferences.language;
    root.dataset.theme = preferences.theme;
    root.dataset.resolvedTheme = resolvedTheme;
    document.title = translate(preferences.language, 'document.title');
  }, [preferences.language, preferences.theme, resolvedTheme]);

  const value = useMemo<PresentationContextValue>(
    () => ({
      addRecent: (identity) =>
        update((current) => ({
          ...current,
          recents: prependIdentity(current.recents, identity, MAX_RECENT_IDENTITIES),
        })),
      locale,
      markNotificationsOpened: () =>
        update((current) => ({ ...current, notificationLastOpenedAt: Date.now() })),
      preferences,
      resolvedTheme,
      setExpanded: (identity, expanded) =>
        update((current) => ({
          ...current,
          expanded: expanded
            ? prependIdentity(current.expanded, identity, MAX_EXPANDED_IDENTITIES)
            : removeIdentity(current.expanded, identity),
        })),
      setExplorerOpen: (explorerOpen) => update((current) => ({ ...current, explorerOpen })),
      setExplorerWidth: (explorerWidth) => update((current) => ({ ...current, explorerWidth })),
      setLanguage: (language) => update((current) => ({ ...current, language })),
      setTheme: (theme) => update((current) => ({ ...current, theme })),
      t: (key, values) => translate(preferences.language, key, values),
      toggleFavorite: (identity) =>
        update((current) => ({
          ...current,
          favorites: includesIdentity(current.favorites, identity)
            ? removeIdentity(current.favorites, identity)
            : prependIdentity(current.favorites, identity, MAX_FAVORITE_IDENTITIES),
        })),
    }),
    [locale, preferences, resolvedTheme, update],
  );

  return <PresentationContext value={value}>{children}</PresentationContext>;
}

export function usePresentation(): PresentationContextValue {
  return use(PresentationContext);
}

function preferredSystemTheme(): ResolvedTheme {
  return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches === true
    ? 'dark'
    : 'light';
}

function prependIdentity(
  current: readonly PreferenceIdentity[],
  identity: PreferenceIdentity,
  limit: number,
): readonly PreferenceIdentity[] {
  return [identity, ...removeIdentity(current, identity)].slice(0, limit);
}

function removeIdentity(
  current: readonly PreferenceIdentity[],
  identity: PreferenceIdentity,
): readonly PreferenceIdentity[] {
  return current.filter((candidate) => !sameIdentity(candidate, identity));
}

function includesIdentity(
  current: readonly PreferenceIdentity[],
  identity: PreferenceIdentity,
): boolean {
  return current.some((candidate) => sameIdentity(candidate, identity));
}

function sameIdentity(left: PreferenceIdentity, right: PreferenceIdentity): boolean {
  return left.kind === right.kind && left.id === right.id;
}
