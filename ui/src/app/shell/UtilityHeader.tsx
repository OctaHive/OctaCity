import { Bell, Boxes, ChevronDown, Search, SunMoon, UserRound } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';

import { CONSOLE_PATHS } from '../routes';
import styles from './UtilityHeader.module.css';

type ReadinessTone = 'checking' | 'ready' | 'unavailable' | 'unreachable';
type ThemeMode = 'system' | 'light' | 'dark';

interface UtilityHeaderProps {
  onOpenSearch: (trigger: HTMLElement) => void;
  readinessLabel: string;
  readinessTone: ReadinessTone;
}

export function UtilityHeader({ onOpenSearch, readinessLabel, readinessTone }: UtilityHeaderProps) {
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  const [theme, setTheme] = useState<ThemeMode>('system');

  useEffect(() => {
    const colorScheme = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
    const applyTheme = () => {
      document.documentElement.dataset.theme = theme;
      document.documentElement.dataset.resolvedTheme =
        theme === 'system' ? (colorScheme?.matches === true ? 'dark' : 'light') : theme;
    };
    applyTheme();
    colorScheme?.addEventListener('change', applyTheme);
    return () => {
      colorScheme?.removeEventListener('change', applyTheme);
      delete document.documentElement.dataset.theme;
      delete document.documentElement.dataset.resolvedTheme;
    };
  }, [theme]);

  return (
    <header className={styles.utilityHeader}>
      <Link aria-label="OctaCity Projects" className={styles.brand} to={CONSOLE_PATHS.projects}>
        <span className={styles.brandMark} aria-hidden="true">
          <Boxes size={21} strokeWidth={1.8} />
        </span>
        <span className={styles.brandText}>
          <strong>OctaCity</strong>
          <small>Operator Console</small>
        </span>
      </Link>

      <button
        aria-haspopup="dialog"
        aria-label="Search resources"
        className={styles.globalSearch}
        onClick={(event) => onOpenSearch(event.currentTarget)}
        type="button"
      >
        <Search aria-hidden="true" size={17} />
        <span>Search resources</span>
        <kbd>Ctrl/⌘ K</kbd>
      </button>

      <div className={styles.headerUtilities}>
        <div
          aria-live="polite"
          className={`${styles.readiness ?? ''} ${styles[`readiness_${readinessTone}`] ?? ''}`}
          role="status"
          title={readinessLabel}
        >
          <span className={styles.readinessDot} aria-hidden="true" />
          <span className={styles.readinessLabel}>{readinessLabel}</span>
        </div>

        <label className={styles.themeControl}>
          <SunMoon aria-hidden="true" size={17} />
          <span className={styles.visuallyHidden}>Theme</span>
          <select
            aria-label="Theme"
            onChange={(event) => setTheme(event.currentTarget.value as ThemeMode)}
            value={theme}
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>

        <div className={styles.popoverAnchor}>
          <button
            aria-expanded={notificationsOpen}
            aria-haspopup="dialog"
            aria-label="Notifications"
            className={styles.iconButton}
            onClick={() => setNotificationsOpen((current) => !current)}
            type="button"
          >
            <Bell aria-hidden="true" size={18} />
          </button>
          {notificationsOpen ? (
            <section
              aria-label="Notification center"
              className={styles.headerPopover}
              role="dialog"
            >
              <strong>Notifications</strong>
              <p>Notification sources are not enabled in this console build.</p>
            </section>
          ) : null}
        </div>

        <details className={styles.operatorMenu}>
          <summary aria-label="Operator menu">
            <UserRound aria-hidden="true" size={18} />
            <span>Operator</span>
            <ChevronDown aria-hidden="true" size={14} />
          </summary>
          <div className={styles.operatorMenuPanel}>
            <strong>Local console settings</strong>
            <label className={styles.operatorLanguage}>
              <span>Language</span>
              <select aria-label="Language" defaultValue="en">
                <option value="en">English</option>
              </select>
            </label>
            <dl>
              <div>
                <dt>Access</dt>
                <dd>Trusted network</dd>
              </div>
              <div>
                <dt>Preferences</dt>
                <dd>Browser local</dd>
              </div>
            </dl>
            <nav aria-label="Operator resources" className={styles.operatorResources}>
              <a href="/api/v1/openapi.json" rel="noreferrer" target="_blank">
                API documentation
              </a>
              <a href="#trusted-network-notice">Deployment information</a>
            </nav>
            <p>No browser identity or session is active.</p>
          </div>
        </details>
      </div>
    </header>
  );
}
