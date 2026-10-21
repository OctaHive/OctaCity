import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueCodeReview } from './codeReviewApi';

/** Current and historical views render the same allowlisted, nullable retained facts. */
export function ReviewObservation({
  observation: o,
}: {
  observation: IssueCodeReview['observation'];
}) {
  const { t, locale } = usePresentation();
  const unknown = t('issues.notReported');
  return (
    <>
      {o?.url ? (
        <a href={o.url} rel="noreferrer noopener" target="_blank">
          {t('issueReviews.open')}
        </a>
      ) : null}
      <p>{t('issueReviews.retained')}</p>
      <dl>
        <dt>{t('issueReviews.author')}</dt>
        <dd>{o?.author ?? unknown}</dd>
        <dt>{t('issueReviews.state')}</dt>
        <dd>{o?.state ? t(`issueReviews.state.${o.state}`) : unknown}</dd>
        <dt>{t('issueReviews.draft')}</dt>
        <dd>{o?.draft == null ? unknown : t(o.draft ? 'common.yes' : 'common.no')}</dd>
        <dt>{t('issueReviews.head')}</dt>
        <dd>{o?.head ?? unknown}</dd>
        <dt>{t('issueReviews.base')}</dt>
        <dd>{o?.base ?? unknown}</dd>
        <dt>{t('issueReviews.files')}</dt>
        <dd>{o?.changed_files ?? unknown}</dd>
        <dt>{t('issueReviews.additions')}</dt>
        <dd>{o?.additions ?? unknown}</dd>
        <dt>{t('issueReviews.deletions')}</dt>
        <dd>{o?.deletions ?? unknown}</dd>
        <dt>{t('issueReviews.review')}</dt>
        <dd>{o?.review ?? unknown}</dd>
        <dt>{t('issueReviews.checks')}</dt>
        <dd>{o?.checks ?? unknown}</dd>
        <dt>{t('issueReviews.observed')}</dt>
        <dd>{o ? new Date(o.observed_at).toLocaleString(locale) : unknown}</dd>
      </dl>
      <p>{t('issueReviews.noAuthority')}</p>
    </>
  );
}
