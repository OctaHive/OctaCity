import { useInfiniteQuery, useQueryClient, type InfiniteData } from '@tanstack/react-query';
import {
  ClipboardList,
  Clock3,
  FolderKanban,
  Hammer,
  MonitorCog,
  Moon,
  Search,
  ServerCog,
  Star,
  Sun,
} from 'lucide-react';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { useNavigate } from 'react-router-dom';

import { RESOURCE_SEARCH_CONSTRAINTS } from '../../../.generated/api/constraints';
import {
  resourceSearchQueryBytes,
  type ResourceSearchApi,
  type ResourceSearchKind,
  type ResourceSearchPage,
  type ResourceSearchResult,
} from '../../api/resourceSearch';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { useOverlayFocus } from '../../shared/useOverlayFocus';
import { queryKeys, RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS } from '../query';
import { usePresentation } from '../presentation/PresentationProvider';
import type { MessageKey } from '../presentation/messages';
import { CONSOLE_PATHS, searchableResourcePath } from '../routes';
import type { PreferenceIdentity } from '../presentation/preferences';
import { resourceKindMessageKeys, type ResourceSearchScope } from './searchScope';
import styles from './CommandCenter.module.css';

interface CommandCenterProps {
  api: ResourceSearchApi;
  onClose: () => void;
  onClearScope?: () => void;
  returnFocus: HTMLElement;
  scope: ResourceSearchScope;
}

interface LocalCommand {
  icon: typeof Search;
  key: string;
  label: string;
  run: () => void;
}

type SearchableIdentity = PreferenceIdentity & { readonly kind: ResourceSearchKind };

const resourceKindOrder: readonly ResourceSearchKind[] = [
  'project',
  'build',
  'agent',
  'agent_pool',
];

/** Keyboard-first bounded resource search and safe local command surface. */
export function CommandCenter({
  api,
  onClearScope,
  onClose,
  returnFocus,
  scope,
}: CommandCenterProps) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { addRecent, locale, preferences, setTheme, t, toggleFavorite } = usePresentation();
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const restoreFocus = useCallback(() => returnFocus.focus(), [returnFocus]);
  const normalizedQuery = query.trim();
  const queryTooLong =
    resourceSearchQueryBytes(normalizedQuery) > RESOURCE_SEARCH_CONSTRAINTS.queryMaximumBytes;
  const waitingForDebounce =
    normalizedQuery !== '' && !queryTooLong && normalizedQuery !== debouncedQuery;
  const globalMode = scope.length === 0;

  useOverlayFocus({
    containerRef: dialogRef,
    initialFocusRef: inputRef,
    onDismiss: onClose,
    priority: true,
    restoreFocus,
  });

  useEffect(() => {
    void queryClient.cancelQueries({ queryKey: queryKeys.resourceSearchRoot });
  }, [normalizedQuery, queryClient, scope]);

  useEffect(() => {
    const nextQuery = queryTooLong ? '' : normalizedQuery;
    const timeout = globalThis.setTimeout(
      () => setDebouncedQuery(nextQuery),
      RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS,
    );
    return () => globalThis.clearTimeout(timeout);
  }, [normalizedQuery, queryTooLong]);

  const search = useInfiniteQuery<
    ResourceSearchPage,
    Error,
    InfiniteData<ResourceSearchPage>,
    ReturnType<typeof queryKeys.resourceSearch>,
    string | null
  >({
    enabled: debouncedQuery !== '' && debouncedQuery === normalizedQuery,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) =>
      api.searchResources(debouncedQuery, scope, pageParam, signal),
    queryKey: queryKeys.resourceSearch(debouncedQuery, scope),
  });
  const results = search.data?.pages.flatMap((page) => page.items) ?? [];

  const goToIdentity = useCallback(
    (identity: SearchableIdentity) => {
      addRecent(identity);
      navigate(searchableResourcePath(identity.kind, identity.id));
      onClose();
    },
    [addRecent, navigate, onClose],
  );
  const runNavigation = useCallback(
    (path: string) => {
      navigate(path);
      onClose();
    },
    [navigate, onClose],
  );
  const navigationCommands = globalMode
    ? navigationItems(t, runNavigation)
    : ([] satisfies readonly LocalCommand[]);
  const presentationCommands = globalMode
    ? presentationItems(t, {
        close: onClose,
        setTheme,
      })
    : ([] satisfies readonly LocalCommand[]);
  const visibleNavigation = filterCommands(navigationCommands, normalizedQuery, locale);
  const visiblePresentation = filterCommands(presentationCommands, normalizedQuery, locale);
  const favoriteIdentities = searchableIdentities(preferences.favorites, scope);
  const recentIdentities = searchableIdentities(preferences.recents, scope);

  function handleKeyboardNavigation(event: KeyboardEvent<HTMLElement>) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const targets = [
      ...(dialogRef.current?.querySelectorAll<HTMLElement>('[data-command-target]') ?? []),
    ];
    if (targets.length === 0) return;
    event.preventDefault();
    const active = document.activeElement;
    const currentIndex = active instanceof HTMLElement ? targets.indexOf(active) : -1;
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    const nextIndex =
      currentIndex < 0
        ? event.key === 'ArrowDown'
          ? 0
          : targets.length - 1
        : (currentIndex + delta + targets.length) % targets.length;
    targets[nextIndex]?.focus();
  }

  return (
    <div className={styles.modalLayer}>
      <button aria-hidden="true" className={styles.modalScrim} onClick={onClose} tabIndex={-1} />
      <section
        aria-labelledby="command-center-title"
        aria-modal="true"
        className={styles.commandCenter}
        onKeyDown={handleKeyboardNavigation}
        ref={dialogRef}
        role="dialog"
      >
        <h2 className={styles.visuallyHidden} id="command-center-title">
          {t('shell.searchResources')}
        </h2>
        <div className={styles.searchRow}>
          <Search aria-hidden="true" size={21} />
          <label>
            <span className={styles.visuallyHidden}>{t('command.query')}</span>
            <input
              aria-invalid={queryTooLong || undefined}
              onChange={(event) => setQuery(event.currentTarget.value)}
              placeholder={t('command.placeholder')}
              ref={inputRef}
              type="search"
              value={query}
            />
          </label>
          <button
            aria-label={t('command.closeCenter')}
            className={styles.escapeButton}
            onClick={onClose}
            type="button"
          >
            <kbd aria-hidden="true">Esc</kbd>
          </button>
        </div>
        <div className={styles.commandScope}>
          <span>{t('command.scope', { scope: scopeLabel(scope, t) })}</span>
          <span>{t('command.shortcutHint')}</span>
          {onClearScope === undefined ? null : (
            <button onClick={onClearScope} type="button">
              {t('command.searchAll')}
            </button>
          )}
        </div>
        {queryTooLong ? (
          <p className={styles.validation} role="alert">
            {t('command.queryTooLong', {
              maximum: RESOURCE_SEARCH_CONSTRAINTS.queryMaximumBytes,
            })}
          </p>
        ) : null}
        <div className={styles.commandBody}>
          {normalizedQuery === '' ? (
            <InitialContent
              favorites={favoriteIdentities}
              navigation={visibleNavigation}
              onOpen={goToIdentity}
              presentation={visiblePresentation}
              recents={recentIdentities}
            />
          ) : queryTooLong ? null : (
            <SearchContent
              commands={visiblePresentation}
              dataAvailable={search.data !== undefined}
              error={search.error}
              fetching={search.isFetching && !search.isFetchingNextPage}
              fetchingNextPage={search.isFetchingNextPage}
              globalMode={globalMode}
              hasNextPage={search.hasNextPage}
              loading={waitingForDebounce || search.isPending}
              navigation={visibleNavigation}
              onFavorite={toggleFavorite}
              onLoadMore={() => void search.fetchNextPage()}
              onOpen={goToIdentity}
              onRetry={search.isFetchNextPageError ? search.fetchNextPage : search.refetch}
              results={results}
            />
          )}
        </div>
        {normalizedQuery === '' ? <p className={styles.commandHint}>{t('command.hint')}</p> : null}
      </section>
    </div>
  );
}

function InitialContent({
  favorites,
  navigation,
  onOpen,
  presentation,
  recents,
}: {
  favorites: readonly SearchableIdentity[];
  navigation: readonly LocalCommand[];
  onOpen: (identity: SearchableIdentity) => void;
  presentation: readonly LocalCommand[];
  recents: readonly SearchableIdentity[];
}) {
  const { t } = usePresentation();
  return (
    <>
      <CommandList commands={navigation} title={t('command.navigate')} />
      <IdentityList
        empty={t('command.noFavorites')}
        icon={<Star aria-hidden="true" size={17} />}
        identities={favorites}
        onOpen={onOpen}
        title={t('command.favorites')}
      />
      <IdentityList
        empty={t('command.noRecents')}
        icon={<Clock3 aria-hidden="true" size={17} />}
        identities={recents}
        onOpen={onOpen}
        title={t('command.recents')}
      />
      <CommandList commands={presentation} title={t('command.commands')} />
    </>
  );
}

function SearchContent({
  commands,
  dataAvailable,
  error,
  fetching,
  fetchingNextPage,
  globalMode,
  hasNextPage,
  loading,
  navigation,
  onFavorite,
  onLoadMore,
  onOpen,
  onRetry,
  results,
}: {
  commands: readonly LocalCommand[];
  dataAvailable: boolean;
  error: Error | null;
  fetching: boolean;
  fetchingNextPage: boolean;
  globalMode: boolean;
  hasNextPage: boolean;
  loading: boolean;
  navigation: readonly LocalCommand[];
  onFavorite: (identity: SearchableIdentity) => void;
  onLoadMore: () => void;
  onOpen: (identity: SearchableIdentity) => void;
  onRetry: () => unknown;
  results: readonly ResourceSearchResult[];
}) {
  const { t } = usePresentation();
  if (loading) return <QueryLoadingNotice label={t('command.searching')} />;
  if (!dataAvailable) {
    return <QueryFailureNotice error={error} onRetry={onRetry} title={t('command.loadFailure')} />;
  }
  const hasLocalMatches = navigation.length > 0 || commands.length > 0;
  return (
    <>
      {globalMode ? <CommandList commands={navigation} title={t('command.navigate')} /> : null}
      {globalMode ? <CommandList commands={commands} title={t('command.commands')} /> : null}
      <QueryBackgroundNotice
        error={error}
        fetching={fetching}
        label={t('command.results')}
        onRetry={onRetry}
      />
      {results.length === 0 && !hasLocalMatches ? (
        <QueryEmptyNotice>{t('command.noMatches')}</QueryEmptyNotice>
      ) : (
        <GroupedResults onFavorite={onFavorite} onOpen={onOpen} results={results} />
      )}
      {hasNextPage ? (
        <button
          className={styles.loadMore}
          data-command-target
          disabled={fetchingNextPage}
          onClick={onLoadMore}
          type="button"
        >
          {fetchingNextPage ? t('command.loadingMore') : t('command.loadMore')}
        </button>
      ) : null}
    </>
  );
}

function GroupedResults({
  onFavorite,
  onOpen,
  results,
}: {
  onFavorite: (identity: SearchableIdentity) => void;
  onOpen: (identity: SearchableIdentity) => void;
  results: readonly ResourceSearchResult[];
}) {
  const { preferences, t } = usePresentation();
  return resourceKindOrder.map((kind) => {
    const grouped = results.filter((result) => result.kind === kind);
    if (grouped.length === 0) return null;
    return (
      <section className={styles.commandGroup} key={kind}>
        <h3>{t(resourceKindMessageKeys[kind])}</h3>
        <ul>
          {grouped.map((result) => {
            const identity = { id: result.id, kind: result.kind } satisfies SearchableIdentity;
            const favorite = includesIdentity(preferences.favorites, identity);
            const favoriteLabel = favorite
              ? t('command.removeFavorite', { label: result.label })
              : t('command.addFavorite', { label: result.label });
            return (
              <li className={styles.resultRow} key={`${result.kind}:${result.id}`}>
                <button
                  aria-label={t('command.openResource', { label: result.label })}
                  className={styles.resultButton}
                  data-command-target
                  onClick={() => onOpen(identity)}
                  type="button"
                >
                  <strong>{result.label}</strong>
                  {result.context === null ? null : <span>{result.context}</span>}
                  <small>{result.id}</small>
                </button>
                <button
                  aria-label={favoriteLabel}
                  aria-pressed={favorite}
                  className={styles.favoriteButton}
                  onClick={() => onFavorite(identity)}
                  title={favoriteLabel}
                  type="button"
                >
                  <Star aria-hidden="true" fill={favorite ? 'currentColor' : 'none'} size={17} />
                </button>
              </li>
            );
          })}
        </ul>
      </section>
    );
  });
}

function CommandList({ commands, title }: { commands: readonly LocalCommand[]; title: string }) {
  if (commands.length === 0) return null;
  return (
    <section className={styles.commandGroup}>
      <h3>{title}</h3>
      <ul>
        {commands.map((command) => {
          const Icon = command.icon;
          return (
            <li key={command.key}>
              <button
                className={styles.commandButton}
                data-command-target
                onClick={command.run}
                type="button"
              >
                <Icon aria-hidden="true" size={18} />
                <span>{command.label}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function IdentityList({
  empty,
  icon,
  identities,
  onOpen,
  title,
}: {
  empty: string;
  icon: ReactNode;
  identities: readonly SearchableIdentity[];
  onOpen: (identity: SearchableIdentity) => void;
  title: string;
}) {
  const { t } = usePresentation();
  return (
    <section className={styles.commandGroup}>
      <h3>
        {icon}
        {title}
      </h3>
      {identities.length === 0 ? (
        <p className={styles.emptyShortcut}>{empty}</p>
      ) : (
        <ul>
          {identities.map((identity) => (
            <li key={`${identity.kind}:${identity.id}`}>
              <button
                className={styles.commandButton}
                data-command-target
                onClick={() => onOpen(identity)}
                title={identity.id}
                type="button"
              >
                <span>{t(resourceKindMessageKeys[identity.kind])}</span>
                <small>{identity.id}</small>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function navigationItems(
  t: (key: MessageKey) => string,
  navigate: (path: string) => void,
): readonly LocalCommand[] {
  return [
    {
      icon: FolderKanban,
      key: 'projects',
      label: t('command.navigateProjects'),
      run: () => navigate(CONSOLE_PATHS.projects),
    },
    {
      icon: Hammer,
      key: 'builds',
      label: t('command.navigateBuilds'),
      run: () => navigate(CONSOLE_PATHS.builds),
    },
    {
      icon: ServerCog,
      key: 'agents',
      label: t('command.navigateAgents'),
      run: () => navigate(CONSOLE_PATHS.agents),
    },
    {
      icon: ClipboardList,
      key: 'audit',
      label: t('command.navigateAudit'),
      run: () => navigate(CONSOLE_PATHS.audit),
    },
  ];
}

function presentationItems(
  t: (key: MessageKey) => string,
  actions: {
    close: () => void;
    setTheme: (theme: 'system' | 'light' | 'dark') => void;
  },
): readonly LocalCommand[] {
  const theme = (key: string, label: MessageKey, value: 'system' | 'light' | 'dark') => ({
    icon: value === 'dark' ? Moon : value === 'light' ? Sun : MonitorCog,
    key,
    label: t(label),
    run: () => {
      actions.setTheme(value);
      actions.close();
    },
  });
  return [
    theme('theme-system', 'command.themeSystem', 'system'),
    theme('theme-light', 'command.themeLight', 'light'),
    theme('theme-dark', 'command.themeDark', 'dark'),
  ];
}

function searchableIdentities(
  identities: readonly PreferenceIdentity[],
  scope: ResourceSearchScope,
): readonly SearchableIdentity[] {
  return identities.filter(
    (identity): identity is SearchableIdentity =>
      identity.kind !== 'build_configuration' &&
      (scope.length === 0 || scope.includes(identity.kind)),
  );
}

function includesIdentity(
  identities: readonly PreferenceIdentity[],
  identity: SearchableIdentity,
): boolean {
  return identities.some(
    (candidate) => candidate.kind === identity.kind && candidate.id === identity.id,
  );
}

function filterCommands(
  commands: readonly LocalCommand[],
  query: string,
  locale: string,
): readonly LocalCommand[] {
  if (query === '') return commands;
  const needle = query.toLocaleLowerCase(locale);
  return commands.filter((command) => command.label.toLocaleLowerCase(locale).includes(needle));
}

function scopeLabel(
  scope: ResourceSearchScope,
  t: (key: MessageKey, values?: Readonly<Record<string, number | string>>) => string,
): string {
  if (scope.length === 0) return t('command.scopeAll');
  const labels = scope.map((kind) => t(resourceKindMessageKeys[kind]));
  return labels.length === 2
    ? t('command.scopePair', { first: labels[0] ?? '', second: labels[1] ?? '' })
    : labels.join(', ');
}
