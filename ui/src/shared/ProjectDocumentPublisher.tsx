import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import type { ProjectDocumentsApi } from '../api/projectDocuments';
import { queryKeys } from '../app/query';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { ConfirmedCommand } from './ConfirmedCommand';

type Request = components['schemas']['PublishProjectDocumentRequest'];
/** Publishes one reviewed immutable input; assigning it to a stage is a separate explicit choice. */
export function ProjectDocumentPublisher({
  api,
  projectId,
}: {
  api: ProjectDocumentsApi;
  projectId: string;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<Request>({ name: '', content: '' });
  const [review, setReview] = useState<{ request: Request; returnFocus: HTMLElement } | null>(null);
  return (
    <fieldset>
      <legend>{t('projectDocument.publish')}</legend>
      <p>{t('projectDocument.help')}</p>
      <label>
        {t('projectDocument.name')}
        <input
          value={draft.name}
          required
          onChange={(event) => setDraft({ ...draft, name: event.target.value })}
        />
      </label>
      <label>
        {t('projectDocument.content')}
        <textarea
          value={draft.content}
          required
          onChange={(event) => setDraft({ ...draft, content: event.target.value })}
        />
      </label>
      <button
        type="button"
        disabled={!draft.name.trim() || !draft.content.trim()}
        onClick={(event) => setReview({ request: { ...draft }, returnFocus: event.currentTarget })}
      >
        {t('projectDocument.review')}
      </button>
      {review ? (
        <ConfirmedCommand
          request={review.request}
          title={review.request.name}
          consequence={review.request.content}
          confirmLabel={t('projectDocument.publish')}
          returnFocus={review.returnFocus}
          queryClient={queryClient}
          invalidations={[
            { queryKey: queryKeys.projectInputDocuments(projectId) },
            { queryKey: queryKeys.audit },
          ]}
          execute={({ request, headers }) => api.publish(projectId, { ...request }, headers)}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('projectDocument.published')}</p>}
        />
      ) : null}
    </fieldset>
  );
}
