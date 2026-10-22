import { IssueRequirements } from './IssueRequirements';
import { queryKeys } from '../../app/query';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent } from '../../shared/QueryStateNotice';
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
      <QueryContent query={list} label={t('issues.revisions')}>
        {(data) => (
          <>
            <ul className={styles.list}>
              {data.items.map((item) => (
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
            {data.next_cursor ? (
              <button onClick={() => setAfter(data.next_cursor)}>{t('issues.nextPage')}</button>
            ) : null}
          </>
        )}
      </QueryContent>
      {version !== null ? (
        <section aria-label={t('issues.historical', { version })}>
          <h3>{t('issues.historical', { version })}</h3>
          <Link to={{ search: revisionSearch(null) }}>{t('issues.closeRevision')}</Link>
          <QueryContent query={body} label={t('issues.historical', { version })}>
            {(data) => (
              <>
                <h4>
                  {data.content_available === false
                    ? t('issues.contentUnavailableTitle')
                    : data.content.title}
                </h4>
                <IssueRequirements issue={data} headingLevel={4} />
              </>
            )}
          </QueryContent>
        </section>
      ) : null}
    </section>
  );
}
