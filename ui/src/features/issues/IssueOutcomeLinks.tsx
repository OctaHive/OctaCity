import { useEffect, useId, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { buildPath } from '../../app/routes';
import { queryKeys } from '../../app/query';
import { formatBytes } from '../../shared/display';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { QueryContent, QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueCodeReviewsApi } from './codeReviewApi';
import type { IssueResultPanelsApi } from './resultPanelsApi';
import { IssueArtifactContent } from './IssueArtifactContent';
import { IssueCodeReviewHistory } from './IssueCodeReviewHistory';
import styles from './Issues.module.css';

type Card = components['schemas']['IssueOutcomeCard'];
type Resource = Exclude<Card['links'][number], { kind: 'unavailable' | 'pull_request' }>;

const resourceLabels = {
  document: 'issueOutcomes.document',
  artifact: 'artifacts.artifact',
  work_artifact: 'artifacts.artifact',
  review: 'issueReviews.open',
} as const;

function selection(link: Resource) {
  if (link.kind === 'review') return `review:${link.review_id}`;
  if (link.kind === 'work_artifact') return `work_artifact:${link.work_id}:${link.artifact_id}`;
  return `${link.kind}:${link.artifact_id}`;
}

/** A link can open only an exact resource still present in the authorized outcome response. */
export function IssueOutcomeLinks({
  card,
  issue,
  reviews,
  panels,
}: {
  card: Card;
  issue: Issue;
  reviews: IssueCodeReviewsApi;
  panels: IssueResultPanelsApi;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const rowKeys = useDraftRowKeys(card.links);
  const renderedResources = new Set<string>();
  const current = params.get('outcome') === card.slot ? params.get('outcome_resource') : null;
  function location(resource?: string) {
    const next = new URLSearchParams(params);
    if (resource === undefined) {
      next.delete('outcome');
      next.delete('outcome_resource');
    } else {
      next.set('outcome', card.slot);
      next.set('outcome_resource', resource);
    }
    return { search: next.toString() };
  }
  return (
    <ul className={styles.outcomeLinks}>
      {card.links.map((link, index) => {
        if (link.kind === 'pull_request') {
          const url =
            card.presentation.kind === 'publication' ? safePullRequestUrl(link.url) : null;
          return url ? (
            <li key={rowKeys[index]}>
              <a href={url.href} rel="noreferrer">
                {t('issueOutcomes.openPullRequest')}
              </a>
            </li>
          ) : null;
        }
        if (link.kind === 'unavailable')
          return (
            <li key={rowKeys[index]}>
              {link.title ? `${link.title} · ` : null}
              <span>{t('issueResults.state.unavailable')}</span>
            </li>
          );
        const id = selection(link);
        if (renderedResources.has(id)) return null;
        renderedResources.add(id);
        const label = link.title ?? t(resourceLabels[link.kind]);
        return (
          <li key={id}>
            <ResourceLink
              title={label}
              target={location(id)}
              close={location()}
              open={id === current}
            >
              <ResourceContent
                link={link}
                issue={issue}
                title={label}
                reviews={reviews}
                panels={panels}
              />
            </ResourceLink>
          </li>
        );
      })}
    </ul>
  );
}

function safePullRequestUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && !url.username && !url.password && !url.search && !url.hash
      ? url
      : null;
  } catch {
    return null;
  }
}

function ResourceLink({
  title,
  target,
  close,
  open,
  children,
}: {
  title: string;
  target: { search: string };
  close: { search: string };
  open: boolean;
  children: React.ReactNode;
}) {
  const { t } = usePresentation();
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const triggerRef = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    if (open) headingRef.current?.focus();
  }, [open]);
  return (
    <>
      <Link ref={triggerRef} to={target} aria-expanded={open}>
        {title}
      </Link>
      {open ? (
        <section className={styles.outcomeResource} aria-labelledby={headingId}>
          <h4 ref={headingRef} id={headingId} tabIndex={-1}>
            {title}
          </h4>
          {children}
          <Link to={close} onClick={() => triggerRef.current?.focus()}>
            {t('common.close')}
          </Link>
        </section>
      ) : null}
    </>
  );
}

function ResourceContent({
  link,
  issue,
  title,
  reviews,
  panels,
}: {
  link: Resource;
  issue: Issue;
  title: string;
  reviews: IssueCodeReviewsApi;
  panels: IssueResultPanelsApi;
}) {
  switch (link.kind) {
    case 'review':
      return <IssueCodeReviewHistory issue={issue} reviewId={link.review_id} api={reviews} />;
    case 'work_artifact':
      return (
        <IssueArtifactContent
          api={panels}
          issue={issue}
          work={link.work_id}
          artifact={{ id: link.artifact_id, name: link.title }}
        />
      );
    default:
      return <ResourceRead api={panels} issue={issue} link={link} title={title} />;
  }
}

function ResourceRead({
  api,
  issue,
  link,
  title,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  link: Extract<Resource, { kind: 'artifact' | 'document' }>;
  title: string;
}) {
  const { t, locale } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueOutcomeResource(
      issue.project_id,
      issue.id,
      link.kind,
      link.artifact_id,
    ),
    gcTime: 0,
    queryFn: async ({ signal }) => {
      if (link.kind === 'artifact') {
        const result = await api.buildArtifact(link.artifact_id, signal);
        if (result.id !== link.artifact_id) throw new Error('Artifact response is invalid');
        return result;
      }
      return api.projectDocument(issue.project_id, link.artifact_id, signal);
    },
  });
  if (query.error)
    return <QueryFailureNotice title={title} error={query.error} onRetry={query.refetch} />;
  return (
    <QueryContent query={query} label={title}>
      {(data) =>
        typeof data === 'string' ? (
          <pre className={styles.prose}>{data}</pre>
        ) : (
          <>
            <p>{data.name}</p>
            <p>
              {data.media_type} · {formatBytes(data.size_bytes, locale)}
            </p>
            <Link to={buildPath(data.build_id)}>{t('issueOutcomes.openBuild')}</Link>
          </>
        )
      }
    </QueryContent>
  );
}
