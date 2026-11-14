import { createPortal } from 'react-dom';
import { ISSUE_DOCUMENT_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { TextDocumentEditor, type TextDocumentDraft } from '../../shared/TextDocumentEditor';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { Issue } from './api';
import type { IssueDocumentsApi } from './documentsApi';
import styles from '../../shared/ProjectDocumentPublisher.module.css';
/** Text and confirmed retry identity live only until this Issue's form is closed. */
export function IssueDocumentDraft({
  api,
  issue,
  initial,
  ready,
  onClose,
}: {
  api: IssueDocumentsApi;
  issue: Issue;
  initial?: TextDocumentDraft;
  ready: boolean;
  onClose: () => void;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(initial ?? { name: '', content: '' });
  const [review, setReview] = useState<{
    request: TextDocumentDraft;
    returnFocus: HTMLElement;
  } | null>(null);
  const [published, setPublished] = useState(false);
  return (
    <TextDocumentEditor
      title={t('issueDocuments.new')}
      draft={draft}
      onChange={setDraft}
      help={t('issueDocuments.help')}
    >
      <div className={styles.toolbar}>
        <button
          className={styles.button}
          type="button"
          disabled={
            !ready ||
            !draft.name.trim() ||
            !draft.content.trim() ||
            new TextEncoder().encode(draft.name).length > bounds.nameMaximumBytes ||
            new TextEncoder().encode(draft.content).length > bounds.contentMaximumBytes
          }
          onClick={(e) => setReview({ request: { ...draft }, returnFocus: e.currentTarget })}
        >
          {t('issueDocuments.reviewPublication')}
        </button>
        <button className={styles.button} type="button" onClick={onClose}>
          {t('projectDocument.cancel')}
        </button>
      </div>
      {review
        ? createPortal(
            <ConfirmedCommand
              ready={ready}
              request={review.request}
              title={t('issueDocuments.reviewPublication')}
              consequence={t('issueDocuments.help')}
              renderReview={<p className={styles.markdown}>{review.request.content}</p>}
              confirmLabel={t('issueDocuments.publish')}
              returnFocus={review.returnFocus}
              queryClient={queryClient}
              invalidations={[
                { queryKey: queryKeys.issueDocuments(issue.project_id, issue.id) },
                { queryKey: queryKeys.audit },
              ]}
              execute={({ request, headers }) =>
                api.publish(issue.project_id, issue.id, { ...request }, headers)
              }
              onSucceeded={() => setPublished(true)}
              onClose={() => (published ? onClose() : setReview(null))}
              renderSuccess={() => <p>{t('issueDocuments.published')}</p>}
            />,
            document.body,
          )
        : null}
    </TextDocumentEditor>
  );
}
