import { useQuery } from '@tanstack/react-query';
import { Download, FileArchive } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { formatBytes, formatEnumLabel, formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { ArtifactPage, BuildResultDiagnosticsApi } from './api';
import styles from './Builds.module.css';

export function BuildArtifacts({
  api,
  buildId,
}: {
  api: BuildResultDiagnosticsApi;
  buildId: string;
}) {
  const artifacts = useQuery({
    queryFn: ({ signal }) => api.listBuildArtifacts(buildId, signal),
    queryKey: queryKeys.buildArtifacts(buildId),
  });
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const downloadControllerRef = useRef<AbortController | null>(null);
  const downloadWindowRef = useRef<Window | null>(null);

  useEffect(
    () => () => {
      downloadControllerRef.current?.abort();
      downloadWindowRef.current?.close();
    },
    [],
  );

  async function download(artifactId: string) {
    if (downloadingId !== null) return;
    const downloadWindow = openDownloadWindow();
    if (downloadWindow === null) {
      setDownloadError('The browser blocked the download window. Allow pop-ups and try again.');
      return;
    }
    const controller = new AbortController();
    downloadControllerRef.current = controller;
    downloadWindowRef.current = downloadWindow;
    setDownloadingId(artifactId);
    setDownloadError(null);
    try {
      const capability = await api.authorizeArtifactDownload(artifactId, controller.signal);
      if (!controller.signal.aborted) launchDownload(capability.get_url, downloadWindow);
    } catch (error) {
      downloadWindow.close();
      if (!controller.signal.aborted) setDownloadError(downloadFailureMessage(error));
    } finally {
      if (controller.signal.aborted) downloadWindow.close();
      else setDownloadingId(null);
      if (downloadControllerRef.current === controller) downloadControllerRef.current = null;
      if (downloadWindowRef.current === downloadWindow) downloadWindowRef.current = null;
    }
  }

  return (
    <section aria-label="Published Artifacts" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Published outputs</p>
          <h2>Artifacts and reports</h2>
        </div>
      </div>
      {artifacts.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label="published outputs" />
      ) : artifacts.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={artifacts.error}
          onRetry={artifacts.refetch}
          title="Published outputs could not be loaded."
        />
      ) : artifacts.data.items.length === 0 ? (
        <QueryEmptyNotice className={styles.diagnosticEmpty}>
          No published Artifacts or reports.
        </QueryEmptyNotice>
      ) : (
        <ArtifactTable
          downloadingId={downloadingId}
          items={artifacts.data.items}
          onDownload={download}
        />
      )}
      {artifacts.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={artifacts.error}
          fetching={artifacts.isFetching}
          label="published outputs data"
          onRetry={artifacts.refetch}
        />
      )}
      {downloadError === null ? null : (
        <p className={styles.inlineFailure} role="alert">
          {downloadError}
        </p>
      )}
    </section>
  );
}

function ArtifactTable({
  downloadingId,
  items,
  onDownload,
}: {
  downloadingId: string | null;
  items: ArtifactPage['items'];
  onDownload: (artifactId: string) => Promise<void>;
}) {
  return (
    <div className={styles.tableScroller}>
      <table aria-label="Published Build outputs" className={styles.diagnosticTable}>
        <thead>
          <tr>
            <th scope="col">Output</th>
            <th scope="col">Type</th>
            <th scope="col">Size</th>
            <th scope="col">Published</th>
            <th scope="col">Content identity</th>
            <th scope="col">Action</th>
          </tr>
        </thead>
        <tbody>
          {items.map((artifact) => {
            const published = formatTimestamp(artifact.published_at_unix_ms);
            const type =
              artifact.output_type.kind === 'artifact'
                ? 'Artifact'
                : `${formatEnumLabel(artifact.output_type.format)} report`;
            return (
              <tr key={artifact.id}>
                <th scope="row">
                  <span className={styles.outputName}>
                    <FileArchive aria-hidden="true" size={15} />
                    {artifact.name}
                  </span>
                  <span className={styles.secondaryMetadata}>{artifact.media_type}</span>
                </th>
                <td>{type}</td>
                <td>{formatBytes(artifact.size_bytes)}</td>
                <td>
                  <time dateTime={published.machine ?? undefined}>{published.display}</time>
                </td>
                <td>
                  <code className={styles.digest}>{artifact.sha256}</code>
                </td>
                <td>
                  <button
                    aria-label={`Download ${artifact.name}`}
                    className={styles.textButton}
                    disabled={downloadingId !== null}
                    onClick={() => void onDownload(artifact.id)}
                    type="button"
                  >
                    <Download aria-hidden="true" size={14} />
                    {downloadingId === artifact.id ? 'Preparing' : 'Download'}
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function downloadFailureMessage(error: unknown): string {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return requestId === null
    ? 'The download could not be started.'
    : `The download could not be started. Request ID: ${requestId}`;
}

function openDownloadWindow(): Window | null {
  const target = window.open('about:blank', '_blank', 'popup');
  if (target === null) return null;
  try {
    target.opener = null;
    const referrerPolicy = target.document.createElement('meta');
    referrerPolicy.name = 'referrer';
    referrerPolicy.content = 'no-referrer';
    target.document.head.append(referrerPolicy);
    return target;
  } catch {
    target.close();
    return null;
  }
}

function launchDownload(value: string, target: Window) {
  const url = new URL(value, window.location.href);
  if (url.protocol !== 'https:' && url.protocol !== 'http:') {
    throw new TypeError('Unsupported Artifact download URL');
  }
  target.location.replace(url.href);
}
