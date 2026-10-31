import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import type { ProjectDocumentsApi } from '../api/projectDocuments';
import { queryKeys } from '../app/query';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { ConfirmedCommand } from './ConfirmedCommand';

/** One confirmed publication/replay boundary shared by Project documents and process input editors. */
export function ProjectDocumentPublication({
  api,
  projectId,
  request,
  returnFocus,
  onClose,
  onPublished,
  confirmLabel,
  successMessage,
  consequence,
}: {
  api: ProjectDocumentsApi;
  projectId: string;
  request: components['schemas']['PublishProjectDocumentRequest'];
  returnFocus: HTMLElement;
  onClose: () => void;
  onPublished?: (document: components['schemas']['ProjectDocumentResource']) => void;
  confirmLabel?: string;
  successMessage?: string;
  consequence?: string;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  return (
    <ConfirmedCommand
      request={request}
      title={request.name}
      consequence={consequence ?? request.content}
      confirmLabel={confirmLabel ?? t('projectDocument.publish')}
      returnFocus={returnFocus}
      queryClient={queryClient}
      invalidations={[
        { queryKey: queryKeys.projectInputDocuments(projectId) },
        { queryKey: queryKeys.audit },
      ]}
      execute={async ({ request, headers }) => {
        const document = await api.publish(projectId, { ...request }, headers);
        onPublished?.(document);
        return document;
      }}
      onClose={onClose}
      renderSuccess={() => <p>{successMessage ?? t('projectDocument.published')}</p>}
    />
  );
}
