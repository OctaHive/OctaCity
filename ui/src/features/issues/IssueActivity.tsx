import { ISSUE_DOCUMENT_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { IssueMaterialReview, type MaterialReview } from './IssueMaterialReview';
import type { IssueDocumentsApi } from './documentsApi';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent, QueryEmptyNotice, QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type {
  IssueActivityApi,
  IssueComment,
  IssueTimelineEntry,
  IssueTimelinePage,
} from './activityApi';
import { IssueCommentForm } from './IssueCommentForm';
import { selectIssueExecution } from './issueExecutionSelection';
import styles from './Issues.module.css';
type MaterialControls = {
  ready: boolean;
  onToggle: (comment: IssueComment, trigger: HTMLElement) => void;
};
/** Bounded safe references; body reads are explicit and operator text conveys no gate authority. */
export function IssueActivity({
  api,
  issue,
  preview = false,
  materials,
  ready = true,
  onRefresh = async () => {},
}: {
  api: IssueActivityApi;
  issue: Issue;
  preview?: boolean;
  materials?: Pick<IssueDocumentsApi, 'editMaterials'>;
  ready?: boolean;
  onRefresh?: () => Promise<void>;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const [after, setAfter] = useState<string | null>(null);
  const [review, setReview] = useState<MaterialReview | null>(null);
  const [parent, setParent] = useState<IssueComment | null>(null);
  const query = useQuery({
    queryKey: queryKeys.issueTimeline(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.timeline(issue.project_id, issue.id, after, signal),
  });
  const controls: MaterialControls | undefined =
    materials && !preview
      ? {
          ready: ready && query.data !== undefined && query.error === null && !query.isFetching,
          onToggle: (comment, returnFocus) => {
            const references = (issue.materials ?? []).map((entry) => entry.reference);
            const selected = references.some(
              (r) => r.kind === 'comment' && r.comment_id === comment.id,
            );
            setReview({
              version: issue.version,
              returnFocus,
              references: selected
                ? references.filter((r) => r.kind !== 'comment' || r.comment_id !== comment.id)
                : [
                    ...references,
                    {
                      kind: 'comment',
                      project_id: issue.project_id,
                      issue_id: issue.id,
                      comment_id: comment.id,
                      body_digest: comment.body_digest,
                    },
                  ],
            });
          },
        }
      : undefined;
  const activity = new URLSearchParams(params);
  activity.set('section', 'activity');
  return (
    <section className={styles.panel} aria-label={t('issues.taskActivity')}>
      <h2>{t('issues.taskActivity')}</h2>
      {!preview ? <p>{t('issues.discussionHelp')}</p> : null}
      {controls ? <p>{t('issues.commentMaterialHelp')}</p> : null}
      <QueryContent query={query} label={t('issues.taskActivity')}>
        {(data) => (
          <>
            <IssueObservation page={data} />
            {data.items.length ? (
              <ol className={styles.list}>
                {data.items.map((entry) => (
                  <ActivityRow
                    key={
                      entry.kind === 'discussion'
                        ? entry.comment.id
                        : entry.kind === 'execution'
                          ? entry.request_id
                          : entry.kind === 'source'
                            ? entry.id
                            : entry.kind + entry.version
                    }
                    entry={entry}
                    api={api}
                    issue={issue}
                    preview={preview}
                    onReply={setParent}
                    materialControls={controls}
                  />
                ))}
              </ol>
            ) : (
              <QueryEmptyNotice>{t('issues.noTimeline')}</QueryEmptyNotice>
            )}
            {!preview ? (
              <div className={styles.actions}>
                {data.next_cursor ? (
                  <button
                    disabled={query.isFetching || query.error !== null}
                    onClick={() => setAfter(data.next_cursor)}
                  >
                    {t('issues.nextPage')}
                  </button>
                ) : null}
              </div>
            ) : null}
          </>
        )}
      </QueryContent>
      {!preview && after ? (
        <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button>
      ) : null}
      {!preview ? (
        <IssueCommentForm
          api={api}
          issue={issue}
          parent={parent}
          onParent={setParent}
          onPosted={() => setAfter(null)}
          ready={query.data !== undefined && query.error === null && !query.isFetching}
        />
      ) : null}
      {review && materials ? (
        <IssueMaterialReview
          api={materials}
          issue={issue}
          review={review}
          ready={controls?.ready ?? false}
          onRefresh={onRefresh}
          onClose={() => setReview(null)}
        />
      ) : null}
      {preview ? (
        <Link to={{ search: activity.toString() }}>{t('issues.timelinePreview')}</Link>
      ) : (
        <button disabled={query.isFetching} onClick={() => void query.refetch()}>
          {t('issues.refreshActivity')}
        </button>
      )}
    </section>
  );
}
/** Independent local/source coverage shared by card sections; never an acceptance verdict. */
export function IssueObservation({
  page,
}: {
  page: Pick<
    IssueTimelinePage,
    'boundary' | 'current_version' | 'source_stale' | 'source_unavailable'
  >;
}) {
  const { t } = usePresentation();
  return (
    <>
      {page.current_version !== page.boundary.issue_version ? (
        <p role="status">
          {t('issues.timelineHistorical', {
            version: page.boundary.issue_version,
            current: page.current_version,
          })}
        </p>
      ) : null}
      {page.source_stale ? <p role="status">{t('issues.sourceStale')}</p> : null}
      {page.source_unavailable ? <p role="status">{t('issues.sourceUnavailable')}</p> : null}
    </>
  );
}
function ActivityRow({
  entry,
  api,
  issue,
  preview,
  onReply,
  materialControls,
}: {
  entry: IssueTimelineEntry;
  api: IssueActivityApi;
  issue: Issue;
  preview: boolean;
  onReply: (parent: IssueComment) => void;
  materialControls: MaterialControls | undefined;
}) {
  const { t, locale } = usePresentation();
  const [params] = useSearchParams();
  const search =
    entry.kind === 'execution'
      ? selectIssueExecution(params, entry.request_id)
      : new URLSearchParams(params);
  search.set('section', entry.kind === 'execution' ? 'runs' : 'activity');
  if (entry.kind === 'revision') search.set('revision', String(entry.version));
  return (
    <li>
      {entry.kind === 'discussion' ? (
        <CommentReference
          api={api}
          issue={issue}
          comment={entry.comment}
          preview={preview}
          onReply={onReply}
          materialControls={materialControls}
        />
      ) : (
        <>
          <time dateTime={new Date(entry.at).toISOString()}>
            {new Date(entry.at).toLocaleString(locale)}
          </time>{' '}
          {entry.kind === 'revision' ? (
            <Link to={{ search: search.toString() }}>
              {t('issues.readRevision', { version: entry.version })}
            </Link>
          ) : entry.kind === 'execution' ? (
            <Link to={{ search: search.toString() }}>
              {t('issues.executionReview', { version: entry.requirements_version })}
            </Link>
          ) : (
            <p>{t(entry.kind === 'status' ? 'issues.statusEvent' : 'issues.sourceEvent')}</p>
          )}
        </>
      )}
    </li>
  );
}
function CommentReference({
  api,
  issue,
  comment,
  preview,
  onReply,
  materialControls,
}: {
  api: IssueActivityApi;
  issue: Issue;
  comment: IssueComment;
  preview: boolean;
  onReply: (parent: IssueComment) => void;
  materialControls: MaterialControls | undefined;
}) {
  const { t, locale } = usePresentation();
  const [open, setOpen] = useState(false);
  const selected = (issue.materials ?? []).some(
    (entry) => entry.reference.kind === 'comment' && entry.reference.comment_id === comment.id,
  );
  return (
    <>
      <p>
        {comment.actor.identity ??
          t(
            comment.actor.kind === 'unauthenticated_management'
              ? 'issues.managementActor'
              : 'issues.unknownActor',
          )}{' '}
        ·{' '}
        <time dateTime={new Date(comment.created_at).toISOString()}>
          {new Date(comment.created_at).toLocaleString(locale)}
        </time>
      </p>
      {comment.parent_id ? (
        <details>
          <summary>{t('issues.rootReply')}</summary>
          <p>{comment.parent_id}</p>
        </details>
      ) : null}
      {!preview ? (
        <>
          <button aria-expanded={open} onClick={() => setOpen(!open)}>
            {t(open ? 'issues.hideComment' : 'issues.readComment')}
          </button>
          {comment.parent_id === null ? (
            <button onClick={() => onReply(comment)}>{t('issues.replyComment')}</button>
          ) : null}
          {materialControls &&
          comment.project_id === issue.project_id &&
          comment.issue_id === issue.id ? (
            <button
              type="button"
              disabled={
                !materialControls.ready ||
                (!selected && (issue.materials ?? []).length >= bounds.materialsMaximumCount)
              }
              onClick={(e) => materialControls.onToggle(comment, e.currentTarget)}
            >
              {t(selected ? 'issues.removeCommentMaterial' : 'issues.includeCommentMaterial', {
                sequence: comment.sequence,
              })}
            </button>
          ) : null}
          {open ? <CommentBody api={api} issue={issue} comment={comment} /> : null}
        </>
      ) : null}
    </>
  );
}
function CommentBody({
  api,
  issue,
  comment,
}: {
  api: IssueActivityApi;
  issue: Issue;
  comment: IssueComment;
}) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueComment(issue.project_id, issue.id, comment.id),
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const result = await api.comment(issue.project_id, issue.id, comment.id, signal);
      const source = result.comment;
      if (
        source.project_id !== issue.project_id ||
        source.issue_id !== issue.id ||
        source.id !== comment.id ||
        source.body_digest !== comment.body_digest ||
        source.body_bytes !== comment.body_bytes
      )
        throw new Error('Issue comment response is invalid');
      return result;
    },
  });
  // A failed private-body refresh must not render previously authorized text as current.
  if (query.error)
    return (
      <QueryFailureNotice
        title={t('issues.commentContent')}
        error={query.error}
        onRetry={query.refetch}
      />
    );
  return (
    <QueryContent query={query} label={t('issues.commentContent')}>
      {(data) => <p className={styles.prose}>{data.body}</p>}
    </QueryContent>
  );
}
