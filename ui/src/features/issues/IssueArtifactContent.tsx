import { useCallback, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryLoadingNotice, QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueResultPanelsApi, IssueArtifact } from './resultPanelsApi';
import styles from './Issues.module.css';

/** Bytes live only in an explicitly opened expansion and are released on close/leave. */
export function IssueArtifactContent({
  api,
  issue,
  work,
  item,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  work: string;
  item: IssueArtifact;
}) {
  const { t } = usePresentation();
  const [preview, setPreview] = useState(false);
  const [download, setDownload] = useState(false);
  return (
    <>
      <button onClick={() => setPreview(!preview)} aria-expanded={preview}>
        {t(preview ? 'issueResults.closePreview' : 'issueResults.preview')}
      </button>
      <button onClick={() => setDownload(!download)} aria-expanded={download}>
        {t(download ? 'issueResults.closeDownload' : 'artifacts.download')}
      </button>
      {preview ? (
        <Expansion api={api} issue={issue} work={work} item={item} mode="preview" />
      ) : null}
      {download ? (
        <Expansion api={api} issue={issue} work={work} item={item} mode="download" />
      ) : null}
    </>
  );
}
function Expansion({
  api,
  issue,
  work,
  item,
  mode,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  work: string;
  item: IssueArtifact;
  mode: 'preview' | 'download';
}) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueArtifactContent(
      issue.project_id,
      issue.id,
      work,
      item.artifact.artifact_id,
      mode,
    ),
    gcTime: 0,
    staleTime: Infinity,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    queryFn: async ({ signal }) => {
      const blob = await api.content(
        issue.project_id,
        issue.id,
        work,
        item.artifact.artifact_id,
        mode,
        signal,
      );
      if (mode === 'download') return { blob, text: null };
      // Only the server-normalized preview representations can enter the DOM.
      const mediaType = blob.type.split(';', 1)[0]?.trim();
      if (mediaType === 'text/plain') return { blob: null, text: await blob.text() };
      if (mediaType === 'image/png') return { blob, text: null };
      throw new Error('Unsupported preview representation');
    },
  });
  return query.isPending ? (
    <QueryLoadingNotice
      label={t(mode === 'preview' ? 'issueResults.preview' : 'artifacts.download')}
    />
  ) : !query.data ? (
    <QueryFailureNotice
      title={t(mode === 'preview' ? 'issueResults.preview' : 'artifacts.download')}
      error={query.error}
      onRetry={query.refetch}
    />
  ) : query.data.text !== null ? (
    <pre className={styles.prose}>{query.data.text}</pre>
  ) : query.data.blob ? (
    <LocalBlob blob={query.data.blob} item={item} mode={mode} />
  ) : null;
}
/** Object URLs are local to a mounted expansion; no provider URL or HTML renderer is used. */
function LocalBlob({
  blob,
  item,
  mode,
}: {
  blob: Blob;
  item: IssueArtifact;
  mode: 'preview' | 'download';
}) {
  const { t } = usePresentation();
  const attach = useCallback(
    (element: HTMLImageElement | HTMLAnchorElement | null) => {
      if (!element) return;
      const url = URL.createObjectURL(blob);
      const attribute = mode === 'preview' ? 'src' : 'href';
      element.setAttribute(attribute, url);
      return () => {
        element.removeAttribute(attribute);
        URL.revokeObjectURL(url);
      };
    },
    [blob, mode],
  );
  return mode === 'preview' ? (
    <img
      ref={attach}
      className={styles.artifactPreview}
      alt={item.metadata?.name ?? t('issueResults.preview')}
    />
  ) : (
    <a ref={attach} download={item.metadata?.name ?? item.artifact.artifact_id}>
      {t('issueResults.save')}
    </a>
  );
}
