import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { issuePath } from '../../app/routes';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { Issue, IssuesApi } from './api';

/** Relationships are independent authorized reads. One child page is retained at a time. */
export function IssueRelationships({ api, issue }: { api: IssuesApi; issue: Issue }) {
  const { t } = usePresentation();
  const [after, setAfter] = useState<string | null>(null);
  const parentId = issue.planning?.parent_id;
  const parent = useQuery({
    queryKey: queryKeys.issue(issue.project_id, parentId ?? ''),
    enabled: !!parentId,
    gcTime: 0,
    queryFn: ({ signal }) => api.get(issue.project_id, parentId!, signal),
  });
  const children = useQuery({
    queryKey: queryKeys.issueChildren(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.children(issue.project_id, issue.id, after, signal),
  });
  return (
    <section aria-label={t('planning.relationships')}>
      <h3>{t('planning.parent')}</h3>
      {!parentId ? (
        <p>{t('planning.noParent')}</p>
      ) : !parent.data ? (
        parent.isPending ? (
          <QueryLoadingNotice label={t('planning.parent')} />
        ) : (
          <QueryFailureNotice
            title={t('planning.parent')}
            error={parent.error}
            onRetry={parent.refetch}
          />
        )
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('planning.parent')}
            error={parent.error}
            fetching={parent.isFetching}
            onRetry={parent.refetch}
          />
          <Link to={issuePath(issue.project_id, parent.data.id)}>
            {parent.data.display_key} · {parent.data.content.title}
          </Link>
        </>
      )}
      <h3>{t('planning.children')}</h3>
      {!children.data ? (
        children.isPending ? (
          <QueryLoadingNotice label={t('planning.children')} />
        ) : (
          <QueryFailureNotice
            title={t('planning.children')}
            error={children.error}
            onRetry={children.refetch}
          />
        )
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('planning.children')}
            error={children.error}
            fetching={children.isFetching}
            onRetry={children.refetch}
          />
          {!children.data.items.length ? (
            <p>{t('planning.noChildren')}</p>
          ) : (
            <ul>
              {children.data.items.map((c) => (
                <li key={c.id}>
                  <Link to={issuePath(issue.project_id, c.id)}>
                    {c.display_key} · {c.title}
                  </Link>
                </li>
              ))}
            </ul>
          )}
          {children.data.next_cursor ? (
            <button
              disabled={children.isFetching || !!children.error}
              onClick={() => setAfter(children.data!.next_cursor)}
            >
              {t('planning.nextChildren')}
            </button>
          ) : null}
        </>
      )}
      {after ? (
        <button disabled={children.isFetching} onClick={() => setAfter(null)}>
          {t('planning.firstChildren')}
        </button>
      ) : null}
      <p>{t('planning.relationshipHelp')}</p>
    </section>
  );
}
