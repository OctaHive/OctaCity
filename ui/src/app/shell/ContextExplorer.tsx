import { ChevronLeft, FolderOpen, Search, Star } from 'lucide-react';
import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import { Link } from 'react-router-dom';

import { usePresentation } from '../presentation/PresentationProvider';
import { CONSOLE_PATHS, searchableResourcePath } from '../routes';
import {
  EXPLORER_MAX_WIDTH,
  EXPLORER_MIN_WIDTH,
  EXPLORER_RESIZE_STEP,
  clampExplorerWidth,
} from '../presentation/preferences';
import { sectionLabel, type ConsoleSection } from './sections';
import { resourceKindMessageKeys, searchScopeForSection } from './searchScope';
import styles from './ContextExplorer.module.css';
import { useOverlayFocus } from '../../shared/useOverlayFocus';

interface ContextExplorerProps {
  children?: ReactNode;
  headingRef: RefObject<HTMLHeadingElement | null>;
  modal: boolean;
  onCollapse: () => void;
  onOpenSearch: (trigger: HTMLElement) => void;
  onWidthChange: (width: number) => void;
  onWidthCommit: (width: number) => void;
  restoreFocus: () => void;
  section: ConsoleSection;
  width: number;
}

export function ContextExplorer({
  children,
  headingRef,
  modal,
  onCollapse,
  onOpenSearch,
  onWidthChange,
  onWidthCommit,
  restoreFocus,
  section,
  width,
}: ContextExplorerProps) {
  const { addRecent, preferences, t } = usePresentation();
  const localizedSection = sectionLabel(section.id, t);
  const explorerRef = useRef<HTMLElement>(null);
  const resizingRef = useRef(false);
  const latestWidthRef = useRef(width);

  useEffect(() => {
    latestWidthRef.current = width;
  }, [width]);

  useOverlayFocus({
    active: modal,
    containerRef: explorerRef,
    initialFocusRef: headingRef,
    onDismiss: onCollapse,
    restoreFocus,
  });

  const resizeFromPointer = (event: PointerEvent<HTMLDivElement>) => {
    if (!resizingRef.current) {
      return;
    }
    const left = explorerRef.current?.getBoundingClientRect().left ?? 0;
    const nextWidth = clampExplorerWidth(event.clientX - left);
    latestWidthRef.current = nextWidth;
    onWidthChange(nextWidth);
  };

  const stopPointerResize = (event: PointerEvent<HTMLDivElement>) => {
    if (!resizingRef.current) {
      return;
    }
    resizingRef.current = false;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    onWidthCommit(latestWidthRef.current);
  };

  const resizeFromKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    const nextWidth = keyboardWidth(event, width);
    if (nextWidth === null) {
      return;
    }
    event.preventDefault();
    latestWidthRef.current = nextWidth;
    onWidthChange(nextWidth);
    onWidthCommit(nextWidth);
  };

  return (
    <aside
      aria-label={`${localizedSection} ${t('common.explorer').toLocaleLowerCase()}`}
      aria-modal={modal || undefined}
      className={modal ? `${styles.explorer} ${styles.modalExplorer}` : styles.explorer}
      ref={explorerRef}
      role={modal ? 'dialog' : undefined}
    >
      <header className={styles.explorerHeader}>
        <div>
          <p>{t('common.explorer')}</p>
          <h2 ref={headingRef} tabIndex={-1}>
            {localizedSection}
          </h2>
        </div>
        <div className={styles.explorerActions}>
          <button
            aria-haspopup="dialog"
            aria-label={t('shell.searchSection', { section: localizedSection })}
            onClick={(event) => onOpenSearch(event.currentTarget)}
            title={t('shell.searchSection', { section: localizedSection })}
            type="button"
          >
            <Search aria-hidden="true" size={17} />
          </button>
          <button
            aria-label={t('shell.collapseSectionExplorer', { section: localizedSection })}
            onClick={onCollapse}
            title={t('shell.collapseExplorer')}
            type="button"
          >
            <ChevronLeft aria-hidden="true" size={18} />
          </button>
        </div>
      </header>

      <div className={styles.explorerBody}>
        {section.explorerMode === 'tools' ? (
          children
        ) : (
          <>
            <section aria-labelledby={`${section.id}-favorites`} className={styles.explorerSection}>
              <h3 id={`${section.id}-favorites`}>
                <Star aria-hidden="true" size={15} />
                {t('common.favorites')}
              </h3>
              <FavoriteLinks
                favorites={preferences.favorites}
                onOpen={addRecent}
                section={section}
              />
            </section>
            <section aria-labelledby={`${section.id}-browse`} className={styles.explorerSection}>
              <h3 id={`${section.id}-browse`}>
                <FolderOpen aria-hidden="true" size={15} />
                {t('common.browse')}
              </h3>
              {children === undefined ? <ExplorerLinks section={section} /> : children}
            </section>
          </>
        )}
      </div>

      {modal ? null : (
        <div
          aria-label={t('shell.resizeSectionExplorer', { section: localizedSection })}
          aria-orientation="vertical"
          aria-valuemax={EXPLORER_MAX_WIDTH}
          aria-valuemin={EXPLORER_MIN_WIDTH}
          aria-valuenow={width}
          className={styles.explorerResizeHandle}
          onKeyDown={resizeFromKeyboard}
          onPointerCancel={stopPointerResize}
          onPointerDown={(event) => {
            resizingRef.current = true;
            latestWidthRef.current = width;
            event.currentTarget.setPointerCapture?.(event.pointerId);
          }}
          onPointerMove={resizeFromPointer}
          onPointerUp={stopPointerResize}
          role="separator"
          tabIndex={0}
        />
      )}
    </aside>
  );
}

function FavoriteLinks({
  favorites,
  onOpen,
  section,
}: {
  favorites: ReturnType<typeof usePresentation>['preferences']['favorites'];
  onOpen: ReturnType<typeof usePresentation>['addRecent'];
  section: ConsoleSection;
}) {
  const { t } = usePresentation();
  const scope = searchScopeForSection(section.id);
  const visible = favorites.filter(
    (identity) => identity.kind !== 'build_configuration' && scope.includes(identity.kind),
  );
  if (visible.length === 0) return <p>{t('common.noFavorites')}</p>;
  return (
    <ul>
      {visible.map((identity) => {
        if (identity.kind === 'build_configuration') return null;
        return (
          <li key={`${identity.kind}:${identity.id}`}>
            <Link
              aria-label={t('command.openResource', {
                label: `${t(resourceKindMessageKeys[identity.kind])} ${identity.id}`,
              })}
              onClick={() => onOpen(identity)}
              title={identity.id}
              to={searchableResourcePath(identity.kind, identity.id)}
            >
              <span>{t(resourceKindMessageKeys[identity.kind])}</span>
              <small>{identity.id}</small>
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

function ExplorerLinks({ section }: { section: ConsoleSection }) {
  const { t } = usePresentation();
  switch (section.id) {
    case 'agents':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.agents}>{t('shell.allAgents')}</Link>
          </li>
          <li>
            <Link to={CONSOLE_PATHS.agentPools}>{t('shell.agentPools')}</Link>
          </li>
        </ul>
      );
    case 'audit':
      return null;
    case 'builds':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.builds}>{t('shell.buildWorkspace')}</Link>
          </li>
        </ul>
      );
    case 'projects':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.projects}>{t('shell.allProjects')}</Link>
          </li>
        </ul>
      );
  }
}

function keyboardWidth(event: KeyboardEvent, width: number): number | null {
  switch (event.key) {
    case 'ArrowLeft':
      return clampExplorerWidth(width - EXPLORER_RESIZE_STEP);
    case 'ArrowRight':
      return clampExplorerWidth(width + EXPLORER_RESIZE_STEP);
    case 'Home':
      return EXPLORER_MIN_WIDTH;
    case 'End':
      return EXPLORER_MAX_WIDTH;
    default:
      return null;
  }
}
