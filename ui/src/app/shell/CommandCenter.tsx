import { useCallback, useRef } from 'react';

import styles from './CommandCenter.module.css';
import { useOverlayFocus } from './useOverlayFocus';

interface CommandCenterProps {
  onClose: () => void;
  returnFocus: HTMLElement;
  scope: string;
}

/** Provides the focus-safe command-center frame until bounded search lands in task 7.5. */
export function CommandCenter({ onClose, returnFocus, scope }: CommandCenterProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const restoreFocus = useCallback(() => returnFocus.focus(), [returnFocus]);
  useOverlayFocus({
    containerRef: dialogRef,
    initialFocusRef: inputRef,
    onDismiss: onClose,
    priority: true,
    restoreFocus,
  });

  return (
    <div className={styles.modalLayer}>
      <button
        aria-label="Close command center"
        className={styles.modalScrim}
        onClick={onClose}
        tabIndex={-1}
      />
      <section
        aria-labelledby="command-center-title"
        aria-modal="true"
        className={styles.commandCenter}
        ref={dialogRef}
        role="dialog"
      >
        <header>
          <div>
            <p>Command center</p>
            <h2 id="command-center-title">Search resources</h2>
          </div>
          <button onClick={onClose} type="button">
            Close
          </button>
        </header>
        <label>
          <span className={styles.visuallyHidden}>Search query</span>
          <input placeholder="Project, Build, Agent, or Agent Pool" ref={inputRef} type="search" />
        </label>
        <p className={styles.commandScope}>Scope: {scope}</p>
        <p className={styles.commandHint}>
          Bounded resource results will appear here when command-center search is enabled.
        </p>
      </section>
    </div>
  );
}
