import { useQuery } from '@tanstack/react-query';
import { Download, FileArchive } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatBytes, formatTimestamp } from '../../shared/display';
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
  const { locale, t } = usePresentation();
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
      setDownloadError(t('artifacts.downloadBlocked'));
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
      if (!controller.signal.aborted) setDownloadError(downloadFailureMessage(error, t));
    } finally {
      if (controller.signal.aborted) downloadWindow.close();
      else setDownloadingId(null);
      if (downloadControllerRef.current === controller) downloadControllerRef.current = null;
      if (downloadWindowRef.current === downloadWindow) downloadWindowRef.current = null;
    }
  }

  return (
    <section aria-label={t('artifacts.label')} className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>{t('artifacts.eyebrow')}</p>
          <h2>{t('artifacts.title')}</h2>
        </div>
      </div>
      {artifacts.isPending ? (
        <QueryLoadingNotice className={styles.statePanel} label={t('artifacts.outputs')} />
      ) : artifacts.data === undefined ? (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={artifacts.error}
          onRetry={artifacts.refetch}
          title={t('artifacts.loadFailure')}
        />
      ) : artifacts.data.items.length === 0 ? (
        <QueryEmptyNotice className={styles.diagnosticEmpty}>
          {t('artifacts.empty')}
        </QueryEmptyNotice>
      ) : (
        <ArtifactTable
          downloadingId={downloadingId}
          items={artifacts.data.items}
          locale={locale}
          onDownload={download}
        />
      )}
      {artifacts.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={artifacts.error}
          fetching={artifacts.isFetching}
          label={t('artifacts.outputsData')}
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
  locale,
  onDownload,
}: {
  downloadingId: string | null;
  items: ArtifactPage['items'];
  locale: string;
  onDownload: (artifactId: string) => Promise<void>;
}) {
  const { t } = usePresentation();
  return (
    <div className={styles.tableScroller}>
      <table aria-label={t('artifacts.outputs')} className={styles.diagnosticTable}>
        <thead>
          <tr>
            <th scope="col">{t('artifacts.output')}</th>
            <th scope="col">{t('artifacts.type')}</th>
            <th scope="col">{t('artifacts.size')}</th>
            <th scope="col">{t('artifacts.published')}</th>
            <th scope="col">{t('artifacts.contentIdentity')}</th>
            <th scope="col">{t('artifacts.action')}</th>
          </tr>
        </thead>
        <tbody>
          {items.map((artifact) => {
            const published = formatTimestamp(artifact.published_at_unix_ms, locale);
            const type =
              artifact.output_type.kind === 'artifact'
                ? t('artifacts.artifact')
                : t('artifacts.reportType', {
                    format: reportFormatLabel(artifact.output_type.format, locale),
                  });
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
                <td>{formatBytes(artifact.size_bytes, locale)}</td>
                <td>
                  <time dateTime={published.machine ?? undefined}>{published.display}</time>
                </td>
                <td>
                  <code className={styles.digest}>{artifact.sha256}</code>
                </td>
                <td>
                  <button
                    aria-label={t('artifacts.downloadNamed', { name: artifact.name })}
                    className={styles.textButton}
                    disabled={downloadingId !== null}
                    onClick={() => void onDownload(artifact.id)}
                    type="button"
                  >
                    <Download aria-hidden="true" size={14} />
                    {downloadingId === artifact.id
                      ? t('artifacts.preparing')
                      : t('artifacts.download')}
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

function reportFormatLabel(format: string, locale: string): string {
  return format
    .replaceAll(/[_-]+/gu, ' ')
    .replaceAll(
      /\p{L}+/gu,
      (word) => `${word[0]?.toLocaleUpperCase(locale) ?? ''}${word.slice(1)}`,
    );
}

function downloadFailureMessage(
  error: unknown,
  t: ReturnType<typeof usePresentation>['t'],
): string {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return requestId === null
    ? t('artifacts.downloadFailure')
    : t('artifacts.downloadFailureWithRequest', { requestId });
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
