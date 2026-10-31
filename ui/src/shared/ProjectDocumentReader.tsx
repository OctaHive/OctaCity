import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import { readExactProjectDocument, type ProjectDocumentsApi } from '../api/projectDocuments';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { queryKeys } from '../app/query';
import { formatTimestamp } from './display';
import { PagedExplorerBranch, useCursorPage } from './PagedExplorerBranch';
import { QueryFailureNotice } from './QueryStateNotice';
import { SelectMenu } from './SelectMenu';
import { useDeadline } from './useDeadline';

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
  const label = t('projectDocument.existing');
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
        {() => (
          <>
            <SelectMenu
              ariaLabel={label}
              value={selected}
              options={[
                { value: '', label: t('projectDocument.choose') },
                ...documents.map((document) => ({
                  value: document.id,
                  label: `${document.name} · ${formatTimestamp(document.published_at, locale).display}`,
                })),
              ]}
              onValueChange={setSelected}
            />
            <ProjectDocumentRead
              key={selectedDocument?.id ?? ''}
              api={api}
              projectId={projectId}
              document={selectedDocument}
              disabled={catalog.isError}
              onLoad={onLoad}
              onLoading={onLoading}
            />
          </>
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
  const expired = useDeadline(document?.expires_at ?? null);
  const read = useMutation({
    gcTime: 0,
    mutationFn: async (selected: NonNullable<typeof document>) => {
      return {
        name: selected.name,
        content: await readExactProjectDocument(api, projectId, selected.artifact),
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
    read.mutate(document, { onSuccess: onLoad, onSettled: () => onLoading(false) });
  };
  return (
    <>
      <button
        type="button"
        disabled={!document || disabled || expired || read.isPending}
        onClick={open}
      >
        {label ?? t('projectDocument.open')}
      </button>
      {expired ? <p role="status">{t('projectDocument.expired')}</p> : null}
      {read.isError ? (
        <QueryFailureNotice
          error={read.error}
          title={label ?? t('projectDocument.open')}
          onRetry={open}
        />
      ) : null}
    </>
  );
}
