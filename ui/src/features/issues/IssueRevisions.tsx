import { queryKeys } from '../../app/query';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import type { Issue, IssuesApi } from './api';
import styles from './Issues.module.css';
export function IssueRevisions({ api, issue }: { api: IssuesApi; issue: Issue }) {
  const { t, locale } = usePresentation();
  const [params] = useSearchParams();
  function revisionSearch(version: number | null) {
    const next = new URLSearchParams(params);
    next.set('section', 'activity');
    if (version === null) next.delete('revision');
    else next.set('revision', String(version));
    return next.toString();
  }
  const selected = Number(params.get('revision'));
  const version = Number.isSafeInteger(selected) && selected > 0 ? selected : null;
  const [after, setAfter] = useState<number | null>(null);
  const list = useQuery({
    queryKey: queryKeys.issueRevisions(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.revisions(issue.project_id, issue.id, after, signal),
  });
  const body = useQuery({
    queryKey: queryKeys.issueRevision(issue.project_id, issue.id, version),
    gcTime: 0,
    enabled: version !== null,
    queryFn: ({ signal }) => api.revision(issue.project_id, issue.id, version!, signal),
  });
  return (
    <section className={styles.panel} aria-label={t('issues.revisions')}>
      <h2>{t('issues.revisions')}</h2>
      {list.isPending ? (
        <QueryLoadingNotice label={t('issues.revisions')} />
      ) : list.data === undefined ? (
        <QueryFailureNotice
          title={t('issues.revisions')}
          error={list.error}
          onRetry={list.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issues.revisions')}
            fetching={list.isFetching}
            error={list.error}
            onRetry={list.refetch}
          />
          <ul className={styles.list}>
            {list.data.items.map((item) => (
              <li key={item.version}>
                <Link
                  aria-current={item.version === version ? 'page' : undefined}
                  to={{ search: revisionSearch(item.version) }}
                >
                  {t('issues.readRevision', { version: item.version })}
                </Link>{' '}
                · {new Date(item.updated_at_unix_ms).toLocaleString(locale)}
              </li>
            ))}
          </ul>
          {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
          {list.data.next_cursor ? (
            <button onClick={() => setAfter(list.data!.next_cursor)}>{t('issues.nextPage')}</button>
          ) : null}
        </>
      )}
      {version !== null ? (
        <section aria-label={t('issues.historical', { version })}>
          <h3>{t('issues.historical', { version })}</h3>
          <Link to={{ search: revisionSearch(null) }}>{t('issues.closeRevision')}</Link>
          {body.isPending ? (
            <QueryLoadingNotice label={t('issues.historical', { version })} />
          ) : body.data === undefined ? (
            <QueryFailureNotice
              title={t('issues.revisions')}
              error={body.error}
              onRetry={body.refetch}
            />
          ) : (
            <>
              <QueryBackgroundNotice
                label={t('issues.revisions')}
                error={body.error}
                fetching={body.isFetching}
                onRetry={body.refetch}
              />
              <h4>{body.data.content.title}</h4>
              <h4>{t('issues.description')}</h4>
              <p className={styles.prose}>
                {body.data.content.description || t('issues.noDescription')}
              </p>
              <h4>{t('issues.acceptance')}</h4>
              <p className={styles.prose}>
                {body.data.content.acceptance || t('issues.noAcceptance')}
              </p>
            </>
          )}
        </section>
      ) : null}
    </section>
  );
}
