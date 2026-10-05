import { useInfiniteQuery } from '@tanstack/react-query';
import { CircleAlert, CircleCheck, X } from 'lucide-react';
import { useCallback, useMemo, useRef, useState, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

import type {
  OperatorAttentionApi,
  OperatorAttentionItem,
  OperatorAttentionPage,
} from '../../api/operatorAttention';
import {
  COMMAND_FEEDBACK_MESSAGE_KEYS,
  useNotifications,
  type SessionActionOutcome,
} from '../notifications/NotificationProvider';
import {
  attentionScope,
  countUnseen,
  latestAttentionTime,
  MAX_ATTENTION_CENTER_PAGES,
  relevantAttentionItems,
} from '../notifications/notificationModel';
import { usePresentation } from '../presentation/PresentationProvider';
import { OPERATOR_ATTENTION_REFRESH_MILLISECONDS, queryKeys } from '../query';
import { searchableResourcePath } from '../routes';
import { useOverlayFocus } from '../../shared/useOverlayFocus';
import { useDismissibleLayer } from '../../shared/useDismissibleLayer';
import { formatTimestamp } from '../../shared/display';
import styles from './NotificationCenter.module.css';

const MAX_VISIBLE_BADGE_COUNT = 99;

interface NotificationCenterProps {
  api: OperatorAttentionApi;
  icon: ReactNode;
  triggerClassName: string | undefined;
}

export function NotificationCenter({ api, icon, triggerClassName }: NotificationCenterProps) {
  const { actionHistory } = useNotifications();
  const { locale, markNotificationsOpened, preferences, t } = usePresentation();
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const scope = useMemo(() => attentionScope(preferences.favorites), [preferences.favorites]);
  const attention = useInfiniteQuery({
    getNextPageParam: (page: OperatorAttentionPage) => page.next_cursor ?? undefined,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => api.listOperatorAttention(scope, pageParam, signal),
    queryKey: queryKeys.operatorAttention(scope.buildIds, scope.agentIds, scope.poolIds),
    refetchInterval: OPERATOR_ATTENTION_REFRESH_MILLISECONDS,
    refetchIntervalInBackground: false,
  });
  const attentionItems = useMemo(
    () => relevantAttentionItems(attention.data?.pages.flatMap((page) => page.items) ?? [], scope),
    [attention.data, scope],
  );
  const unseenCount = countUnseen(
    actionHistory,
    attentionItems,
    preferences.notificationLastOpenedAt,
  );
  const dismiss = useCallback(() => setOpen(false), []);

  useOverlayFocus({
    active: open,
    containerRef: panelRef,
    initialFocusRef: closeRef,
    onDismiss: dismiss,
    restoreFocus: () => triggerRef.current?.focus(),
  });
  useDismissibleLayer({
    active: open,
    layerRef: panelRef,
    onDismiss: dismiss,
    triggerRef,
  });

  const toggle = () => {
    if (!open) markNotificationsOpened();
    setOpen(!open);
  };
  const canLoadMore =
    attention.hasNextPage === true &&
    (attention.data?.pages.length ?? 0) < MAX_ATTENTION_CENTER_PAGES;

  return (
    <div className={styles.anchor}>
      <button
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={
          unseenCount === 0
            ? t('notification.label')
            : t('notification.labelWithCount', { count: unseenCount })
        }
        className={triggerClassName}
        onClick={toggle}
        ref={triggerRef}
        type="button"
      >
        {icon}
        {unseenCount === 0 ? null : (
          <span aria-hidden="true" className={styles.badge}>
            {unseenCount > MAX_VISIBLE_BADGE_COUNT ? `${MAX_VISIBLE_BADGE_COUNT}+` : unseenCount}
          </span>
        )}
      </button>

      {open ? (
        <section
          aria-label={t('notification.center')}
          className={styles.panel}
          ref={panelRef}
          role="dialog"
          tabIndex={-1}
        >
          <header className={styles.header}>
            <span>
              <strong>{t('notification.label')}</strong>
              <small>{t('notification.browserLocal')}</small>
            </span>
            <button
              aria-label={t('notification.close')}
              onClick={dismiss}
              ref={closeRef}
              type="button"
            >
              <X aria-hidden="true" size={17} />
            </button>
          </header>

          <div className={styles.content}>
            <NotificationGroup title={t('notification.sessionActions')}>
              {actionHistory.length === 0 ? (
                <p className={styles.empty}>{t('notification.noSessionActions')}</p>
              ) : (
                <ul className={styles.items}>
                  {actionHistory.map((item) => (
                    <SessionActionItem item={item} key={item.id} locale={locale} />
                  ))}
                </ul>
              )}
            </NotificationGroup>

            <NotificationGroup title={t('notification.attention')}>
              {attention.isPending ? (
                <p className={styles.empty} role="status">
                  {t('notification.loading')}
                </p>
              ) : null}
              {attention.isError && attentionItems.length === 0 ? (
                <div className={styles.error} role="alert">
                  <span>{t('notification.loadFailure')}</span>
                  <button onClick={() => void attention.refetch()} type="button">
                    {t('query.retry')}
                  </button>
                </div>
              ) : null}
              {attention.isError && attentionItems.length > 0 ? (
                <p className={styles.stale} role="status">
                  {t('notification.stale')}
                </p>
              ) : null}
              {!attention.isPending && !attention.isError && attentionItems.length === 0 ? (
                <p className={styles.empty}>{t('notification.empty')}</p>
              ) : null}
              {attentionItems.length === 0 ? null : (
                <ul className={styles.items}>
                  {attentionItems.map((item) => (
                    <AttentionItem item={item} key={item.id} locale={locale} />
                  ))}
                </ul>
              )}
              {canLoadMore ? (
                <button
                  className={styles.loadMore}
                  disabled={attention.isFetchingNextPage}
                  onClick={() => void attention.fetchNextPage()}
                  type="button"
                >
                  {attention.isFetchingNextPage
                    ? t('notification.loadingMore')
                    : t('notification.loadMore')}
                </button>
              ) : null}
            </NotificationGroup>
          </div>
        </section>
      ) : null}
    </div>
  );
}

function NotificationGroup({ children, title }: { children: ReactNode; title: string }) {
  return (
    <section className={styles.group}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function SessionActionItem({ item, locale }: { item: SessionActionOutcome; locale: string }) {
  const { t } = usePresentation();
  const timestamp = formatTimestamp(item.occurredAtUnixMs, locale);
  return (
    <li className={styles.item} data-tone={item.kind}>
      {item.kind === 'success' ? (
        <CircleCheck aria-hidden="true" size={18} />
      ) : (
        <CircleAlert aria-hidden="true" size={18} />
      )}
      <span>
        <strong>{item.title}</strong>
        <small>{t(COMMAND_FEEDBACK_MESSAGE_KEYS[item.kind])}</small>
        <time dateTime={timestamp.machine ?? undefined}>{timestamp.display}</time>
      </span>
    </li>
  );
}

function AttentionItem({ item, locale }: { item: OperatorAttentionItem; locale: string }) {
  const { t } = usePresentation();
  const resolved = item.resolved_at_unix_ms !== null;
  const latestAt = latestAttentionTime(item);
  const timestamp = formatTimestamp(latestAt, locale);
  const content = (
    <>
      <span className={styles.itemHeading}>
        <strong>{item.summary}</strong>
        <em>
          {resolved
            ? t('notification.resolvedCondition')
            : t(attentionSeverityMessageKeys[item.severity])}
        </em>
      </span>
      <small>{item.code}</small>
      {resolved ? null : <small>{t('notification.activeCondition')}</small>}
      <time dateTime={timestamp.machine ?? undefined}>{timestamp.display}</time>
    </>
  );
  return (
    <li className={styles.item} data-tone={resolved ? 'resolved' : item.severity}>
      {resolved ? (
        <CircleCheck aria-hidden="true" size={18} />
      ) : (
        <CircleAlert aria-hidden="true" size={18} />
      )}
      {item.target === null ? (
        <span>{content}</span>
      ) : (
        <Link to={searchableResourcePath(item.target.kind, item.target.id)}>{content}</Link>
      )}
    </li>
  );
}

const attentionSeverityMessageKeys = {
  critical: 'notification.severityCritical',
  warning: 'notification.severityWarning',
} as const;
