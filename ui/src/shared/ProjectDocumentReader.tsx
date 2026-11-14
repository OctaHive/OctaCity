import { useMutation } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import { readExactProjectDocument, type ProjectDocumentsApi } from '../api/projectDocuments';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { queryKeys } from '../app/query';
import { formatTimestamp } from './display';
import { PagedExplorerBranch, useCursorPage } from './PagedExplorerBranch';
import { QueryFailureNotice } from './QueryStateNotice';
import { useDeadline } from './useDeadline';
import styles from './ProjectDocumentPublisher.module.css';

type ExactDocumentSelection = Pick<
  components['schemas']['ProjectDocumentResource'],
  'name' | 'artifact'
> & {
  expires_at?: number;
};

/** Reads bytes only on explicit request; private content is not kept in the query cache. */
export function ProjectDocumentReader({
  api,
  projectId,
  onLoad,
  onLoading,
  disabled = false,
}: {
  api: ProjectDocumentsApi;
  projectId: string;
  onLoad: (document: components['schemas']['ProjectDocumentResource'], content: string) => void;
  onLoading: (loading: boolean) => void;
  disabled?: boolean;
}) {
  const { t, locale } = usePresentation();
  const catalog = useCursorPage(
    queryKeys.projectInputDocuments(projectId),
    async (cursor, signal) => {
      const page = await api.list(projectId, cursor, signal);
      return {
        ...page,
        items: page.items.map((document) => ({ ...document, id: document.artifact.artifact_id })),
      };
    },
  );
  const label = t('projectDocument.manage');
  return (
    <>
      <PagedExplorerBranch
        query={catalog}
        empty={t('projectDocument.empty')}
        labels={{
          loading: label,
          failure: label,
          stale: label,
          loadingMore: label,
          loadMore: label,
        }}
      >
        {(documents) => (
          <ul className={styles.list}>
            {documents
              .filter((document) => document.project_id === projectId)
              .map((document) => (
                <li className={styles.card} key={document.id}>
                  <div>
                    <h3>{document.name}</h3>
                    <p className={styles.metadata}>
                      {formatTimestamp(document.published_at, locale).display}
                    </p>
                  </div>
                  <ProjectDocumentRead
                    api={api}
                    projectId={projectId}
                    document={document}
                    disabled={disabled || catalog.isError}
                    onLoad={(loaded) => onLoad(document, loaded.content)}
                    onLoading={onLoading}
                    label={t('projectDocument.view', { name: document.name })}
                  />
                </li>
              ))}
          </ul>
        )}
      </PagedExplorerBranch>
    </>
  );
}

/** Reads an exact selection independently of catalog pagination; the owner enforces current access/expiry. */
export function ProjectDocumentRead({
  api,
  projectId,
  document,
  disabled,
  onLoad,
  onLoading,
  label,
}: {
  api: ProjectDocumentsApi;
  projectId: string;
  document: ExactDocumentSelection | undefined;
  disabled: boolean;
  onLoad: (document: components['schemas']['PublishProjectDocumentRequest']) => void;
  onLoading: (loading: boolean) => void;
  label?: string;
}) {
  const { t } = usePresentation();
  const readLabel = label ?? t('projectDocument.view', { name: document?.name ?? '' });
  const expired = useDeadline(document?.expires_at ?? null);
  const read = useMutation({
    gcTime: 0,
    // Release the caller's busy state even if this catalog row disappears mid-read.
    onSettled: () => onLoading(false),
    mutationFn: async (selected: NonNullable<typeof document>) => {
      return {
        name: selected.name,
        content: (await readExactProjectDocument(api, projectId, selected.artifact)).content,
      };
    },
  });
  const open = () => {
    if (
      !document ||
      disabled ||
      (document.expires_at !== undefined && Date.now() >= document.expires_at) ||
      read.isPending
    )
      return;
    onLoading(true);
    read.mutate(document, { onSuccess: onLoad });
  };
  return (
    <>
      <button
        className={styles.button}
        type="button"
        disabled={!document || disabled || expired || read.isPending}
        onClick={open}
      >
        {readLabel}
      </button>
      {expired ? <p role="status">{t('projectDocument.expired')}</p> : null}
      {read.isError ? (
        <QueryFailureNotice error={read.error} title={readLabel} onRetry={open} />
      ) : null}
    </>
  );
}
