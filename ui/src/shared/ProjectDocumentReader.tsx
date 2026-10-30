import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import type { ProjectDocumentsApi } from '../api/projectDocuments';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { queryKeys } from '../app/query';
import { formatTimestamp } from './display';
import { PagedExplorerBranch, useCursorPage } from './PagedExplorerBranch';
import { QueryFailureNotice } from './QueryStateNotice';
import { SelectMenu } from './SelectMenu';

/** Reads bytes only on explicit request; private content is not kept in the query cache. */
export function ProjectDocumentReader({
  api,
  projectId,
  onLoad,
  onLoading,
}: {
  api: ProjectDocumentsApi;
  projectId: string;
  onLoad: (document: components['schemas']['PublishProjectDocumentRequest']) => void;
  onLoading: (loading: boolean) => void;
}) {
  const { t, locale } = usePresentation();
  const [selected, setSelected] = useState('');
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
  const documents =
    catalog.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.project_id === projectId) ?? [];
  const selectedDocument = documents.find((item) => item.id === selected);
  const read = useMutation({
    gcTime: 0,
    mutationFn: async (document: NonNullable<typeof selectedDocument>) => {
      const result = await api.read(projectId, document.id);
      if (result.project_id !== projectId || result.artifact.artifact_id !== document.id)
        throw new Error('Project document response is invalid');
      return { name: document.name, content: result.content };
    },
  });
  const label = t('projectDocument.existing');
  const open = () => {
    if (!selectedDocument) return;
    onLoading(true);
    read.mutate(selectedDocument, { onSuccess: onLoad, onSettled: () => onLoading(false) });
  };
  return (
    <>
      <PagedExplorerBranch
        query={catalog}
        empty={t('triage.noDocuments')}
        labels={{
          loading: label,
          failure: label,
          stale: label,
          loadingMore: label,
          loadMore: label,
        }}
      >
        {() => (
          <>
            <SelectMenu
              ariaLabel={label}
              value={selected}
              disabled={read.isPending}
              options={[
                { value: '', label: t('factory.none') },
                ...documents.map((document) => ({
                  value: document.id,
                  label: `${document.name} · ${formatTimestamp(document.published_at, locale).display}`,
                })),
              ]}
              onValueChange={setSelected}
            />
            <button
              type="button"
              disabled={!selectedDocument || catalog.isError || read.isPending}
              onClick={open}
            >
              {t('projectDocument.open')}
            </button>
          </>
        )}
      </PagedExplorerBranch>
      {read.isError ? (
        <QueryFailureNotice error={read.error} title={t('projectDocument.open')} onRetry={open} />
      ) : null}
    </>
  );
}
