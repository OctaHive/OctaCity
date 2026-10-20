import { useQuery } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueSourcesApi } from './sourceApi';
import { IssueSourceAttachment } from './IssueSourceAttachment';
import { IssueSourceObservation } from './IssueSourceObservation';
import { IssueSourceMapping } from './IssueSourceMapping';
import type { WorkflowResource } from '../../api/issueWorkflows';
import styles from './Issues.module.css';

/** Independent Source observation; a provider outage does not disable local task controls. */
export function IssueSource({
  issue,
  api,
  workflow,
}: {
  issue: Issue;
  api: IssueSourcesApi;
  workflow: WorkflowResource | undefined;
}) {
  const { t } = usePresentation();
  const [mode, setMode] = useState<'attachment' | 'mapping' | null>(null);
  const [generation, setGeneration] = useState(0);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const review = useQuery({
    queryKey: queryKeys.issueSynchronization(issue.project_id, issue.id),
    gcTime: 0,
    queryFn: ({ signal }) => api.review(issue.project_id, issue.id, signal),
  });
  const choices = useQuery({
    queryKey: queryKeys.issueSourceChoices(issue.project_id, issue.id),
    gcTime: 0,
    enabled: mode !== null,
    queryFn: ({ signal }) => api.choices(issue.project_id, issue.id, signal),
  });
  const ready =
    review.data?.issue_version === issue.version &&
    choices.data?.issue_version === issue.version &&
    !review.isFetching &&
    !review.error &&
    !choices.isFetching &&
    !choices.error;
  async function reload() {
    const [current, available] = await Promise.all([review.refetch(), choices.refetch()]);
    if (current.data && available.data && !current.error && !available.error) {
      setMode(current.data.link ? 'mapping' : 'attachment');
      setGeneration((v) => v + 1);
    }
  }
  return (
    <section className={styles.panel} aria-label={t('issueSource.title')}>
      <h2>{t('issueSource.title')}</h2>
      {review.isPending ? (
        <QueryLoadingNotice label={t('issueSource.title')} />
      ) : !review.data ? (
        <QueryFailureNotice
          title={t('issueSource.title')}
          error={review.error}
          onRetry={review.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issueSource.title')}
            fetching={review.isFetching}
            error={review.error}
            onRetry={review.refetch}
          />
          <p>{review.data.link ? review.data.link.selection.item.item : t('issueSource.local')}</p>
          <button
            type="button"
            ref={toggleRef}
            disabled={review.isFetching || review.error !== null}
            onClick={() =>
              setMode(mode === null ? (review.data?.link ? 'mapping' : 'attachment') : null)
            }
          >
            {t(mode === null ? 'issueSource.configure' : 'command.abandon')}
          </button>
          <IssueSourceObservation
            issue={issue}
            api={api}
            review={review.data}
            ready={
              !review.isFetching && !review.error && review.data.issue_version === issue.version
            }
            fallbackFocusRef={toggleRef}
            onReload={review.refetch}
          />
          {mode !== null ? (
            choices.isPending ? (
              <QueryLoadingNotice label={t('issueSource.profile')} />
            ) : !choices.data ? (
              <QueryFailureNotice
                title={t('issueSource.profile')}
                error={choices.error}
                onRetry={choices.refetch}
              />
            ) : (
              <>
                <QueryBackgroundNotice
                  label={t('issueSource.profile')}
                  fetching={choices.isFetching}
                  error={choices.error}
                  onRetry={choices.refetch}
                />
                {mode === 'attachment' ? (
                  <IssueSourceAttachment
                    key={generation}
                    issue={issue}
                    api={api}
                    review={review.data}
                    choices={choices.data.items}
                    ready={ready}
                    onReload={reload}
                    fallbackFocusRef={toggleRef}
                  />
                ) : (
                  <IssueSourceMapping
                    key={generation}
                    issue={issue}
                    api={api}
                    review={review.data}
                    choices={choices.data.items}
                    workflow={workflow}
                    ready={ready}
                    onReload={reload}
                    fallbackFocusRef={toggleRef}
                  />
                )}
              </>
            )
          ) : null}
        </>
      )}
    </section>
  );
}
