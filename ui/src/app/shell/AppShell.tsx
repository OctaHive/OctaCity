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
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { Link, Outlet, useLocation } from 'react-router-dom';

import type { ReadinessProbe } from '../../api/readiness';
import { queryKeys, READINESS_REFRESH_MILLISECONDS } from '../query';
import { CommandCenter } from './CommandCenter';
import { ContextExplorer } from './ContextExplorer';
import { isNarrowWorkbench, observeNarrowWorkbench, useNarrowWorkbench } from './layout';
import { loadExplorerWidth, saveExplorerWidth } from './preferences';
import {
  GLOBAL_RESOURCE_SEARCH_SCOPE,
  searchScopeForSection,
  type ResourceSearchScope,
} from './searchScope';
import { consoleSections, sectionForPath, type ConsoleSectionId } from './sections';
import styles from './Shell.module.css';
import { UtilityHeader } from './UtilityHeader';

const sectionIcons = {
  agents: ServerCog,
  audit: ClipboardList,
  builds: Hammer,
  projects: FolderKanban,
} satisfies Record<ConsoleSectionId, typeof FolderKanban>;

interface AppShellProps {
  explorerContent?: Partial<Record<ConsoleSectionId, ReactNode>>;
  readinessProbe: ReadinessProbe;
}

interface SearchOverlayState {
  returnFocus: HTMLElement;
  scope: ResourceSearchScope;
}

export function AppShell({ explorerContent = {}, readinessProbe }: AppShellProps) {
  const location = useLocation();
  const activeSection = sectionForPath(location.pathname);
  const narrowWorkbench = useNarrowWorkbench();
  const [explorerOpen, setExplorerOpen] = useState(() => !isNarrowWorkbench());
  const [explorerWidth, setExplorerWidth] = useState(loadExplorerWidth);
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
    ? 'Checking readiness'
    : readinessLabels[readinessState];
  const readinessTone = readiness.isPending ? 'checking' : readinessState;
  const shellStyle = { '--explorer-width': `${explorerWidth}px` } as CSSProperties;
  const shellClassName = [
    styles.shell,
    explorerOpen ? null : styles.explorerCollapsed,
    narrowWorkbench ? styles.narrowWorkbench : null,
  ]
    .filter((className): className is string => className !== null)
    .join(' ');
  const closeSearch = useCallback(() => setSearchOverlay(null), []);
  const restoreExplorerFocus = useCallback(() => {
    globalThis.requestAnimationFrame(() => explorerOpenButtonRef.current?.focus());
  }, []);

  useEffect(() => {
    return observeNarrowWorkbench((narrow) => {
      if (narrow) {
        setExplorerOpen(false);
      }
    });
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
    setExplorerOpen(true);
  }, []);

  const collapseExplorer = useCallback(() => {
    setExplorerOpen(false);
    globalThis.requestAnimationFrame(() => explorerOpenButtonRef.current?.focus());
  }, []);

  const selectSection = () => {
    setExplorerOpen(true);
  };

  return (
    <div className={shellClassName} style={shellStyle}>
      <a className={styles.skipLink} href="#console-content">
        Skip to content
      </a>
      <UtilityHeader
        onOpenSearch={(trigger) =>
          setSearchOverlay({ returnFocus: trigger, scope: GLOBAL_RESOURCE_SEARCH_SCOPE })
        }
        readinessLabel={readinessLabel}
        readinessTone={readinessTone}
      />

      <aside className={styles.sectionRail}>
        <nav aria-label="Primary sections">
          {consoleSections.map((section) => {
            const Icon = sectionIcons[section.id];
            const active = section.id === activeSection.id;
            return (
              <Link
                aria-current={active ? 'page' : undefined}
                className={active ? `${styles.railLink} ${styles.activeRailLink}` : styles.railLink}
                key={section.id}
                onClick={selectSection}
                title={section.label}
                to={section.path}
              >
                <Icon aria-hidden="true" size={22} strokeWidth={1.7} />
                <span>{section.label}</span>
              </Link>
            );
          })}
        </nav>
        {!explorerOpen ? (
          <button
            aria-label={`Open ${activeSection.label} explorer`}
            className={styles.openExplorerButton}
            onClick={openExplorer}
            ref={explorerOpenButtonRef}
            title="Open explorer"
            type="button"
          >
            <PanelLeftOpen aria-hidden="true" size={21} />
            <span>Explorer</span>
          </button>
        ) : null}
      </aside>

      {explorerOpen ? (
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
            onWidthCommit={(width) => setExplorerWidth(saveExplorerWidth(width))}
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
          aria-label="Security notice"
          id="trusted-network-notice"
        >
          <ShieldAlert aria-hidden="true" size={18} strokeWidth={1.9} />
          <p>
            <strong>Trusted network only.</strong> This console is unauthenticated and must not be
            exposed to an untrusted network.
          </p>
        </aside>

        <main className={styles.content} id="console-content" tabIndex={-1}>
          <Outlet />
        </main>
      </div>

      {searchOverlay === null ? null : (
        <CommandCenter
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
      )}
    </div>
  );
}

const readinessLabels = {
  ready: 'Server ready',
  unavailable: 'Server unavailable',
  unreachable: 'Server unreachable',
} as const;
