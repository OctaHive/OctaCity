import { ChevronLeft, FolderOpen, Search, Star } from 'lucide-react';
import { useEffect, useRef, type KeyboardEvent, type PointerEvent, type RefObject } from 'react';
import { Link } from 'react-router-dom';

import { CONSOLE_PATHS } from '../routes';
import {
  EXPLORER_MAX_WIDTH,
  EXPLORER_MIN_WIDTH,
  EXPLORER_RESIZE_STEP,
  clampExplorerWidth,
} from './preferences';
import type { ConsoleSection } from './sections';
import styles from './ContextExplorer.module.css';
import { useOverlayFocus } from './useOverlayFocus';

interface ContextExplorerProps {
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
      aria-label={`${section.label} explorer`}
      aria-modal={modal || undefined}
      className={modal ? `${styles.explorer} ${styles.modalExplorer}` : styles.explorer}
      ref={explorerRef}
      role={modal ? 'dialog' : undefined}
    >
      <header className={styles.explorerHeader}>
        <div>
          <p>Explorer</p>
          <h2 ref={headingRef} tabIndex={-1}>
            {section.label}
          </h2>
        </div>
        <div className={styles.explorerActions}>
          <button
            aria-haspopup="dialog"
            aria-label={`Search ${section.label}`}
            onClick={(event) => onOpenSearch(event.currentTarget)}
            title={`Search ${section.label}`}
            type="button"
          >
            <Search aria-hidden="true" size={17} />
          </button>
          <button
            aria-label={`Collapse ${section.label} explorer`}
            onClick={onCollapse}
            title="Collapse explorer"
            type="button"
          >
            <ChevronLeft aria-hidden="true" size={18} />
          </button>
        </div>
      </header>

      <div className={styles.explorerBody}>
        <section aria-labelledby={`${section.id}-favorites`} className={styles.explorerSection}>
          <h3 id={`${section.id}-favorites`}>
            <Star aria-hidden="true" size={15} />
            Favorites
          </h3>
          <p>No favorites yet.</p>
        </section>
        <section aria-labelledby={`${section.id}-browse`} className={styles.explorerSection}>
          <h3 id={`${section.id}-browse`}>
            <FolderOpen aria-hidden="true" size={15} />
            Browse
          </h3>
          <ExplorerLinks section={section} />
        </section>
      </div>

      {modal ? null : (
        <div
          aria-label={`Resize ${section.label} explorer`}
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

function ExplorerLinks({ section }: { section: ConsoleSection }) {
  switch (section.id) {
    case 'agents':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.agents}>All Agents</Link>
          </li>
          <li>
            <Link to={CONSOLE_PATHS.agentPools}>Agent Pools</Link>
          </li>
        </ul>
      );
    case 'audit':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.audit}>Audit filters</Link>
          </li>
        </ul>
      );
    case 'builds':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.builds}>Build workspace</Link>
          </li>
        </ul>
      );
    case 'projects':
      return (
        <ul>
          <li>
            <Link to={CONSOLE_PATHS.projects}>All Projects</Link>
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
