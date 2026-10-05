import { Bell, Boxes, ChevronDown, Monitor, Moon, Search, Sun, UserRound } from 'lucide-react';
import { lazy, Suspense, useRef, useState } from 'react';
import { Link } from 'react-router-dom';

import type { OperatorAttentionApi } from '../../api/operatorAttention';
import { usePresentation } from '../presentation/PresentationProvider';
import { CONSOLE_PATHS } from '../routes';
import type { Language, ThemeMode } from '../presentation/preferences';
import { SelectMenu, type SelectMenuOption } from '../../shared/SelectMenu';
import { useDismissibleLayer } from '../../shared/useDismissibleLayer';
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
  const [operatorOpen, setOperatorOpen] = useState(false);
  const operatorTriggerRef = useRef<HTMLButtonElement>(null);
  const operatorPanelRef = useRef<HTMLDivElement>(null);
  const themeOptions: readonly SelectMenuOption<ThemeMode>[] = [
    { icon: <Sun aria-hidden="true" size={18} />, label: t('theme.light'), value: 'light' },
    { icon: <Moon aria-hidden="true" size={18} />, label: t('theme.dark'), value: 'dark' },
    { icon: <Monitor aria-hidden="true" size={18} />, label: t('theme.system'), value: 'system' },
  ];
  const languageOptions: readonly SelectMenuOption<Language>[] = [
    { label: t('language.english'), value: 'en' },
    { label: t('language.russian'), value: 'ru' },
  ];

  useDismissibleLayer({
    active: operatorOpen,
    layerRef: operatorPanelRef,
    onDismiss: () => setOperatorOpen(false),
    triggerRef: operatorTriggerRef,
  });

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

        <SelectMenu
          ariaLabel={t('theme.label')}
          className={styles.themeControl}
          compact
          onValueChange={setTheme}
          options={themeOptions}
          value={preferences.theme}
        />

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

        <div
          className={styles.operatorMenu}
          onKeyDown={(event) => {
            if (event.key !== 'Escape' || event.defaultPrevented) return;
            event.preventDefault();
            setOperatorOpen(false);
            operatorTriggerRef.current?.focus();
          }}
        >
          <button
            aria-expanded={operatorOpen}
            aria-haspopup="dialog"
            aria-label={t('operator.menu')}
            className={styles.operatorMenuTrigger}
            onClick={() => setOperatorOpen((current) => !current)}
            ref={operatorTriggerRef}
            type="button"
          >
            <UserRound aria-hidden="true" size={18} />
            <span>{t('common.operator')}</span>
            <ChevronDown aria-hidden="true" size={14} />
          </button>
          {operatorOpen ? (
            <div
              aria-label={t('operator.menu')}
              className={styles.operatorMenuPanel}
              ref={operatorPanelRef}
              role="dialog"
            >
              <strong>{t('operator.localSettings')}</strong>
              <div className={styles.operatorLanguage}>
                <span>{t('language.label')}</span>
                <SelectMenu
                  ariaLabel={t('language.label')}
                  onValueChange={setLanguage}
                  options={languageOptions}
                  value={preferences.language}
                />
              </div>
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
              </nav>
              <p>{t('operator.noIdentity')}</p>
            </div>
          ) : null}
        </div>
      </div>
    </header>
  );
}
