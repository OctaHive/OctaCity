import { useQuery } from '@tanstack/react-query';
import {
  ClipboardList,
  FolderKanban,
  Hammer,
  PanelLeftOpen,
  ServerCog,
  ShieldAlert,
} from 'lucide-react';
import {
  useCallback,
  useEffect,
  lazy,
  useRef,
  useState,
  Suspense,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { Link, Outlet, useLocation } from 'react-router-dom';

import type { ReadinessProbe } from '../../api/readiness';
import type { ResourceSearchApi } from '../../api/resourceSearch';
import type { OperatorAttentionApi } from '../../api/operatorAttention';
import { queryKeys, READINESS_REFRESH_MILLISECONDS } from '../query';
import { ContextExplorer } from './ContextExplorer';
import { usePresentation } from '../presentation/PresentationProvider';
import { useNarrowWorkbench } from './layout';
import {
  GLOBAL_RESOURCE_SEARCH_SCOPE,
  searchScopeForSection,
  type ResourceSearchScope,
} from './searchScope';
import { consoleSections, sectionForPath, sectionLabel, type ConsoleSectionId } from './sections';
import styles from './Shell.module.css';
import { UtilityHeader } from './UtilityHeader';

const CommandCenter = lazy(async () => {
  const module = await import('./CommandCenter');
  return { default: module.CommandCenter };
});

const sectionIcons = {
  agents: ServerCog,
  audit: ClipboardList,
  builds: Hammer,
  projects: FolderKanban,
} satisfies Record<ConsoleSectionId, typeof FolderKanban>;

interface AppShellProps {
  explorerContent?: Partial<Record<ConsoleSectionId, ReactNode>>;
  operatorAttentionApi: OperatorAttentionApi;
  readinessProbe: ReadinessProbe;
  resourceSearchApi: ResourceSearchApi;
}

interface SearchOverlayState {
  returnFocus: HTMLElement;
  scope: ResourceSearchScope;
}

export function AppShell({
  explorerContent = {},
  operatorAttentionApi,
  readinessProbe,
  resourceSearchApi,
}: AppShellProps) {
  const location = useLocation();
  const {
    preferences,
    setExplorerOpen,
    setExplorerWidth: persistExplorerWidth,
    t,
  } = usePresentation();
  const activeSection = sectionForPath(location.pathname);
  const activeSectionLabel = sectionLabel(activeSection.id, t);
  const narrowWorkbench = useNarrowWorkbench();
  const [narrowExplorerOpen, setNarrowExplorerOpen] = useState(false);
  const [explorerWidth, setExplorerWidth] = useState(preferences.explorerWidth);
  const [searchOverlay, setSearchOverlay] = useState<SearchOverlayState | null>(null);
  const explorerHeadingRef = useRef<HTMLHeadingElement>(null);
  const focusExplorerOnOpenRef = useRef(false);
  const explorerOpenButtonRef = useRef<HTMLButtonElement>(null);
  const readiness = useQuery({
    queryFn: ({ signal }) => readinessProbe(signal),
    queryKey: queryKeys.readiness,
    refetchInterval: READINESS_REFRESH_MILLISECONDS,
    refetchIntervalInBackground: false,
  });
  const readinessState = readiness.data ?? 'unreachable';
  const readinessLabel = readiness.isPending
    ? t('readiness.checking')
    : t(readinessMessageKeys[readinessState]);
  const readinessTone = readiness.isPending ? 'checking' : readinessState;
  const explorerAvailable = activeSection.explorerMode !== null;
  const explorerOpen = narrowWorkbench ? narrowExplorerOpen : preferences.explorerOpen;
  const explorerVisible = explorerAvailable && explorerOpen;
  const shellStyle = { '--explorer-width': `${explorerWidth}px` } as CSSProperties;
  const shellClassName = [
    styles.shell,
    explorerVisible ? null : styles.explorerCollapsed,
    narrowWorkbench ? styles.narrowWorkbench : null,
  ]
    .filter((className): className is string => className !== null)
    .join(' ');
  const closeSearch = useCallback(() => setSearchOverlay(null), []);
  const restoreExplorerFocus = useCallback(() => {
    globalThis.requestAnimationFrame(() => explorerOpenButtonRef.current?.focus());
  }, []);

  useEffect(() => {
    if (explorerOpen && focusExplorerOnOpenRef.current) {
      focusExplorerOnOpenRef.current = false;
      explorerHeadingRef.current?.focus();
    }
  }, [explorerOpen]);

  useEffect(() => {
    const openCommandCenter = (event: globalThis.KeyboardEvent) => {
      if (
        event.key.toLowerCase() === 'k' &&
        (event.metaKey || event.ctrlKey) &&
        searchOverlay === null
      ) {
        event.preventDefault();
        const returnFocus =
          document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
        setSearchOverlay({ returnFocus, scope: GLOBAL_RESOURCE_SEARCH_SCOPE });
      }
    };
    globalThis.addEventListener('keydown', openCommandCenter);
    return () => globalThis.removeEventListener('keydown', openCommandCenter);
  }, [searchOverlay]);

  const openExplorer = useCallback(() => {
    focusExplorerOnOpenRef.current = true;
    if (narrowWorkbench) setNarrowExplorerOpen(true);
    else setExplorerOpen(true);
  }, [narrowWorkbench, setExplorerOpen]);

  const collapseExplorer = useCallback(() => {
    if (narrowWorkbench) setNarrowExplorerOpen(false);
    else setExplorerOpen(false);
    globalThis.requestAnimationFrame(() => explorerOpenButtonRef.current?.focus());
  }, [narrowWorkbench, setExplorerOpen]);

  return (
    <div className={shellClassName} style={shellStyle}>
      <a className={styles.skipLink} href="#console-content">
        {t('shell.skipToContent')}
      </a>
      <UtilityHeader
        operatorAttentionApi={operatorAttentionApi}
        onOpenSearch={(trigger) =>
          setSearchOverlay({ returnFocus: trigger, scope: GLOBAL_RESOURCE_SEARCH_SCOPE })
        }
        readinessLabel={readinessLabel}
        readinessTone={readinessTone}
      />

      <aside className={styles.sectionRail}>
        <nav aria-label={t('shell.primarySections')}>
          {consoleSections.map((section) => {
            const Icon = sectionIcons[section.id];
            const active = section.id === activeSection.id;
            const label = sectionLabel(section.id, t);
            return (
              <Link
                aria-current={active ? 'page' : undefined}
                className={active ? `${styles.railLink} ${styles.activeRailLink}` : styles.railLink}
                key={section.id}
                onClick={() => {
                  if (narrowWorkbench) setNarrowExplorerOpen(false);
                  else setExplorerOpen(section.explorerMode !== null);
                }}
                title={label}
                to={section.path}
              >
                <Icon aria-hidden="true" size={22} strokeWidth={1.7} />
                <span>{label}</span>
              </Link>
            );
          })}
        </nav>
        {explorerAvailable && !explorerOpen ? (
          <button
            aria-label={t('shell.openSectionExplorer', { section: activeSectionLabel })}
            className={styles.openExplorerButton}
            onClick={openExplorer}
            ref={explorerOpenButtonRef}
            title={t('shell.openExplorer')}
            type="button"
          >
            <PanelLeftOpen aria-hidden="true" size={21} />
            <span>{t('common.explorer')}</span>
          </button>
        ) : null}
      </aside>

      {explorerVisible ? (
        <>
          <button
            aria-hidden="true"
            className={styles.explorerBackdrop}
            onClick={collapseExplorer}
            tabIndex={-1}
            type="button"
          />
          <ContextExplorer
            headingRef={explorerHeadingRef}
            modal={narrowWorkbench}
            onCollapse={collapseExplorer}
            onOpenSearch={(trigger) =>
              setSearchOverlay({
                returnFocus: trigger,
                scope: searchScopeForSection(activeSection.id),
              })
            }
            onWidthChange={setExplorerWidth}
            onWidthCommit={(width) => {
              persistExplorerWidth(width);
              setExplorerWidth(width);
            }}
            restoreFocus={restoreExplorerFocus}
            section={activeSection}
            width={explorerWidth}
          >
            {explorerContent[activeSection.id]}
          </ContextExplorer>
        </>
      ) : null}

      <div className={styles.workspace}>
        <aside
          className={styles.securityBanner}
          aria-label={t('security.label')}
          id="trusted-network-notice"
        >
          <ShieldAlert aria-hidden="true" size={18} strokeWidth={1.9} />
          <p>
            <strong>{t('security.title')}</strong> {t('security.description')}
          </p>
        </aside>

        <main className={styles.content} id="console-content" tabIndex={-1}>
          <Outlet />
        </main>
      </div>

      {searchOverlay === null ? null : (
        <Suspense fallback={<span role="status">{t('route.loadingView')}</span>}>
          <CommandCenter
            api={resourceSearchApi}
            onClose={closeSearch}
            {...(searchOverlay.scope.length === 0
              ? {}
              : {
                  onClearScope: () =>
                    setSearchOverlay((current) =>
                      current === null ? null : { ...current, scope: GLOBAL_RESOURCE_SEARCH_SCOPE },
                    ),
                })}
            returnFocus={searchOverlay.returnFocus}
            scope={searchOverlay.scope}
          />
        </Suspense>
      )}
    </div>
  );
}

const readinessMessageKeys = {
  ready: 'readiness.ready',
  unavailable: 'readiness.unavailable',
  unreachable: 'readiness.unreachable',
} as const;
