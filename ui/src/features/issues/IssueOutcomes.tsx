import { useId } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { IssueOutcomeLinks } from './IssueOutcomeLinks';
import type { IssueResultPanelsApi } from './resultPanelsApi';
import type { IssueCodeReviewsApi } from './codeReviewApi';
import type { Issue, IssuesApi } from './api';
import { formatIssuePriority } from './issuePriority';
import styles from './Issues.module.css';

type Card = components['schemas']['IssueOutcomeCard'];
type Triage = Extract<Card['presentation'], { kind: 'triage' }>;
const coverageLabels = {
  complete: 'issueOutcomes.coverageComplete',
  partial: 'issueOutcomes.coveragePartial',
  unknown: 'triageResult.coverageUnknown',
  not_checked: 'issueOutcomes.searchNotVerified',
} satisfies Record<NonNullable<Triage['duplicate_coverage']>, MessageKey>;

const sizeLabels = {
  small: 'triageResult.value.small',
  medium: 'triageResult.value.medium',
  large: 'triageResult.value.large',
  unknown: 'triageResult.value.unknown',
} satisfies Record<string, MessageKey>;

const componentLabels = {
  server: 'triageResult.value.server',
  console: 'triageResult.value.console',
  agent: 'triageResult.value.agent',
  infrastructure: 'triageResult.value.infrastructure',
  documentation: 'triageResult.value.documentation',
  other: 'triageResult.value.other',
  unknown: 'triageResult.value.unknown',
} satisfies Record<string, MessageKey>;

const verdictLabels = {
  approved: 'issueOutcomes.approved',
  changes_required: 'issueOutcomes.changesRequired',
  needs_review: 'triageResult.value.needs_review',
} satisfies Record<Extract<Card['presentation'], { kind: 'review' }>['verdict'], MessageKey>;

const titles = {
  triage: 'factory.issueCardTriage',
  specifications: 'factory.issueCardSpecifications',
  review: 'factory.issueCardReview',
} satisfies Record<Exclude<Card['presentation']['kind'], 'custom'>, MessageKey>;

/** The server owns current slot selection. This view reads only accepted display fields. */
export function IssueOutcomes({
  api,
  issue,
  reviews,
  panels,
}: {
  api: Pick<IssuesApi, 'outcomes'>;
  issue: Issue;
  reviews: IssueCodeReviewsApi;
  panels: IssueResultPanelsApi;
}) {
  const { t } = usePresentation();
  const title = t('issueOutcomes.title');
  const query = useQuery({
    queryKey: queryKeys.issueOutcomes(issue.project_id, issue.id, issue.applicability_version),
    gcTime: 0,
    queryFn: ({ signal }) => api.outcomes(issue.project_id, issue.id, signal),
  });
  if (query.error)
    return <QueryFailureNotice title={title} error={query.error} onRetry={query.refetch} />;
  if (!query.data?.items.length) return null;
  return (
    <section className={styles.panel} aria-label={title}>
      <h2>{title}</h2>
      {query.data.items.map((card) => (
        <OutcomeCard key={card.slot} card={card} issue={issue} reviews={reviews} panels={panels} />
      ))}
    </section>
  );
}

function OutcomeCard({
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
  const headingId = useId();
  const { presentation } = card;
  const title = presentation.kind === 'custom' ? presentation.title : t(titles[presentation.kind]);
  return (
    <article aria-labelledby={headingId} className={styles.outcomeCard}>
      <h3 id={headingId}>{title}</h3>
      <p className={styles.prose}>{card.conclusion}</p>
      {presentation.kind === 'triage' ? <TriageFacts value={presentation} /> : null}
      {presentation.kind === 'review' ? (
        <OutcomeFacts
          facts={[['factory.issueCardVerdict', t(verdictLabels[presentation.verdict])]]}
        />
      ) : null}
      {card.links.length ? (
        <IssueOutcomeLinks issue={issue} reviews={reviews} panels={panels} card={card} />
      ) : null}
      <Link to={`/factory/${encodeURIComponent(card.run_id)}`}>{t('issues.openRun')}</Link>
    </article>
  );
}

function TriageFacts({ value }: { value: Triage }) {
  const { t } = usePresentation();
  function vocabularyLabel(value: string, labels: Partial<Record<string, MessageKey>>) {
    return Object.hasOwn(labels, value) ? t(labels[value]!) : value;
  }
  const facts: [MessageKey, string | undefined][] = [
    ['factory.decision', t(`triageResult.value.${value.decision}`)],
    [
      'factory.issueCardDuplicateCoverage',
      value.duplicate_coverage ? t(coverageLabels[value.duplicate_coverage]) : undefined,
    ],
    [
      'issueOutcomes.recommendedPriority',
      value.recommended_priority === undefined
        ? undefined
        : formatIssuePriority(value.recommended_priority, t),
    ],
    [
      'issueOutcomes.recommendedSize',
      value.recommended_size ? vocabularyLabel(value.recommended_size, sizeLabels) : undefined,
    ],
    [
      'triageResult.component',
      value.component ? vocabularyLabel(value.component, componentLabels) : undefined,
    ],
  ];
  return <OutcomeFacts facts={facts} />;
}

function OutcomeFacts({ facts }: { facts: readonly [MessageKey, string | undefined][] }) {
  const { t } = usePresentation();
  return (
    <dl className={styles.outcomeFacts}>
      {facts.map(([label, value]) =>
        value === undefined ? null : (
          <div key={label}>
            <dt>{t(label)}</dt>
            <dd>{value}</dd>
          </div>
        ),
      )}
    </dl>
  );
}
