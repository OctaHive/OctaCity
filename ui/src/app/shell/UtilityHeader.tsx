import { Bell, Boxes, ChevronDown, Search, SunMoon, UserRound } from 'lucide-react';
import { lazy, Suspense } from 'react';
import { Link } from 'react-router-dom';

import type { OperatorAttentionApi } from '../../api/operatorAttention';
import { usePresentation } from '../presentation/PresentationProvider';
import { CONSOLE_PATHS } from '../routes';
import type { Language, ThemeMode } from '../presentation/preferences';
import styles from './UtilityHeader.module.css';

const NotificationCenter = lazy(async () => {
  const module = await import('./NotificationCenter');
  return { default: module.NotificationCenter };
});

type ReadinessTone = 'checking' | 'ready' | 'unavailable' | 'unreachable';
interface UtilityHeaderProps {
  onOpenSearch: (trigger: HTMLElement) => void;
  operatorAttentionApi: OperatorAttentionApi;
  readinessLabel: string;
  readinessTone: ReadinessTone;
}

export function UtilityHeader({
  onOpenSearch,
  operatorAttentionApi,
  readinessLabel,
  readinessTone,
}: UtilityHeaderProps) {
  const { preferences, setLanguage, setTheme, t } = usePresentation();

  return (
    <header className={styles.utilityHeader}>
      <Link aria-label={t('brand.home')} className={styles.brand} to={CONSOLE_PATHS.projects}>
        <span className={styles.brandMark} aria-hidden="true">
          <Boxes size={21} strokeWidth={1.8} />
        </span>
        <span className={styles.brandText}>
          <strong>OctaCity</strong>
          <small>{t('brand.product')}</small>
        </span>
      </Link>

      <button
        aria-haspopup="dialog"
        aria-label={t('shell.searchResources')}
        className={styles.globalSearch}
        onClick={(event) => onOpenSearch(event.currentTarget)}
        type="button"
      >
        <Search aria-hidden="true" size={17} />
        <span>{t('shell.searchResources')}</span>
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
          <span className={styles.visuallyHidden}>{t('theme.label')}</span>
          <select
            aria-label={t('theme.label')}
            onChange={(event) => setTheme(event.currentTarget.value as ThemeMode)}
            value={preferences.theme}
          >
            <option value="system">{t('theme.system')}</option>
            <option value="light">{t('theme.light')}</option>
            <option value="dark">{t('theme.dark')}</option>
          </select>
        </label>

        <Suspense
          fallback={
            <button
              aria-label={t('notification.label')}
              className={styles.iconButton}
              disabled
              type="button"
            >
              <Bell aria-hidden="true" size={18} />
            </button>
          }
        >
          <NotificationCenter
            api={operatorAttentionApi}
            icon={<Bell aria-hidden="true" size={18} />}
            triggerClassName={styles.iconButton}
          />
        </Suspense>

        <details className={styles.operatorMenu}>
          <summary aria-label={t('operator.menu')}>
            <UserRound aria-hidden="true" size={18} />
            <span>{t('common.operator')}</span>
            <ChevronDown aria-hidden="true" size={14} />
          </summary>
          <div className={styles.operatorMenuPanel}>
            <strong>{t('operator.localSettings')}</strong>
            <label className={styles.operatorLanguage}>
              <span>{t('language.label')}</span>
              <select
                aria-label={t('language.label')}
                onChange={(event) => setLanguage(event.currentTarget.value as Language)}
                value={preferences.language}
              >
                <option value="en">{t('language.english')}</option>
                <option value="ru">{t('language.russian')}</option>
              </select>
            </label>
            <dl>
              <div>
                <dt>{t('operator.access')}</dt>
                <dd>{t('operator.trustedNetwork')}</dd>
              </div>
              <div>
                <dt>{t('operator.preferences')}</dt>
                <dd>{t('operator.browserLocal')}</dd>
              </div>
            </dl>
            <nav aria-label={t('operator.resources')} className={styles.operatorResources}>
              <a href="/api/v1/openapi.json" rel="noreferrer" target="_blank">
                {t('operator.apiDocumentation')}
              </a>
              <a href="#trusted-network-notice">{t('operator.deploymentInformation')}</a>
            </nav>
            <p>{t('operator.noIdentity')}</p>
          </div>
        </details>
      </div>
    </header>
  );
}
