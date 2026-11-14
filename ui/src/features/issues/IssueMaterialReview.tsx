import { createPortal } from 'react-dom';
import { IssueMaterialSummary, materialKey } from './IssueMaterialSummary';
import { useQueryClient } from '@tanstack/react-query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { Issue } from './api';
import type { IssueMaterialReference, IssueDocumentsApi } from './documentsApi';
export type MaterialReview = {
  references: IssueMaterialReference[];
  version: number;
  returnFocus: HTMLElement;
};
/** One frozen full-set replacement; current Issue revision and source access are still enforced by the server. */
export function IssueMaterialReview({
  api,
  issue,
  review,
  ready,
  onRefresh,
  onClose,
}: {
  api: Pick<IssueDocumentsApi, 'editMaterials'>;
  issue: Issue;
  review: MaterialReview;
  ready: boolean;
  onRefresh: () => Promise<void>;
  onClose: () => void;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  return createPortal(
    <ConfirmedCommand
      title={t('issueDocuments.reviewSelection')}
      request={{ references: review.references }}
      version={review.version}
      ready={ready && review.version === issue.version}
      returnFocus={review.returnFocus}
      queryClient={queryClient}
      confirmLabel={t('issueDocuments.saveSelection')}
      consequence={t('issueDocuments.selectionHelp')}
      renderReview={
        <ul>
          {review.references.map((r) => (
            <li key={materialKey(r)}>
              <IssueMaterialSummary reference={r} />
            </li>
          ))}
        </ul>
      }
      execute={({ request, headers }) =>
        api.editMaterials(
          issue.project_id,
          issue.id,
          { references: [...request.references] },
          requireVersionedMutationHeaders(headers),
        )
      }
      invalidations={[
        { queryKey: queryKeys.issueRoot(issue.project_id, issue.id) },
        { queryKey: queryKeys.audit },
      ]}
      onRefresh={onRefresh}
      onClose={onClose}
      renderSuccess={() => <p>{t('issueDocuments.selectionSaved')}</p>}
    />,
    document.body,
  );
}
