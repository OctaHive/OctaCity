import { IssueResultProducer } from './IssueResultProducer';
import { IssueArtifactContent } from './IssueArtifactContent';
import { useQuery } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatBytes } from '../../shared/display';
import { SafeFactoryMetadata } from '../../shared/FactoryDiagnosticRecord';
import { IssueResultPage } from './IssueResultPage';
import type { Issue } from './api';
import type { IssueResultPanelsApi } from './resultPanelsApi';
import styles from './Issues.module.css';
/** Metadata pages never expand bytes, fetch thumbnails or use provider URLs. */
export function IssueArtifacts({
  api,
  issue,
  work,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  work: string;
}) {
  const { t, locale } = usePresentation();
  const [params, setParams] = useSearchParams();
  const after = params.get('artifact_after');
  const query = useQuery({
    queryKey: queryKeys.issueArtifacts(issue.project_id, issue.id, work, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.artifacts(issue.project_id, issue.id, work, after, signal),
  });
  return (
    <IssueResultPage
      title={t('issueResults.executionArtifacts')}
      query={query}
      after={after}
      onPage={(cursor) => {
        const next = new URLSearchParams(params);
        if (cursor) next.set('artifact_after', cursor);
        else next.delete('artifact_after');
        setParams(next);
      }}
    >
      {(page) => (
        <>
          {!page.items.length && page.coverage === 'partial' ? (
            <p>{t('issueResults.emptyArtifacts')}</p>
          ) : null}
          {(['original_input', 'output', 'report', 'screenshot'] as const).map((group) => {
            const items = page.items.filter((item) => item.group === group);
            return items.length ? (
              <section key={group}>
                <h4>{t(`issueResults.group.${group}`)}</h4>
                <ul className={styles.list}>
                  {items.map((item) => (
                    <li key={item.artifact.artifact_id}>
                      <h5>{item.metadata?.name ?? t('issueResults.unavailableArtifact')}</h5>
                      <p>{t(`issueResults.availability.${item.availability}`)}</p>
                      <p>
                        {item.metadata?.media_type ?? t('issues.notReported')} ·{' '}
                        {formatBytes(item.artifact.encoded_size, locale)}
                      </p>
                      {item.metadata ? (
                        <dl>
                          <dt>{t('issueResults.origin')}</dt>
                          <dd>{t(`issueResults.origin.${item.metadata.origin}`)}</dd>
                          <dt>{t('issueResults.published')}</dt>
                          <dd>
                            {item.metadata.published_at === null
                              ? t('issues.notReported')
                              : new Date(item.metadata.published_at).toLocaleString(locale)}
                          </dd>
                          <dt>{t('issueResults.expires')}</dt>
                          <dd>
                            {item.metadata.expires_at === null
                              ? t('issueResults.noExpiry')
                              : new Date(item.metadata.expires_at).toLocaleString(locale)}
                          </dd>
                        </dl>
                      ) : null}
                      {item.availability === 'available' &&
                      item.metadata &&
                      !query.error &&
                      page.coverage === 'partial' ? (
                        <IssueArtifactContent
                          key={item.artifact.artifact_id + page.run_version}
                          api={api}
                          issue={issue}
                          work={work}
                          item={item}
                        />
                      ) : null}
                      <details>
                        <summary>{t('issueResults.provenance')}</summary>
                        <SafeFactoryMetadata
                          value={item.artifact}
                          fields={['artifact_id', 'content_digest', 'encoded_size']}
                        />
                        <SafeFactoryMetadata value={item} fields={['record', 'subject']} />
                        <IssueResultProducer producer={item.producer} />
                      </details>
                    </li>
                  ))}
                </ul>
              </section>
            ) : null;
          })}
        </>
      )}
    </IssueResultPage>
  );
}
