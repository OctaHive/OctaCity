import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { Issue } from './api';
import styles from './Issues.module.css';

/** Current and historical requirements share the same explicit content availability. */
export function IssueRequirements({
  issue,
  headingLevel = 2,
}: {
  issue: Issue;
  headingLevel?: 2 | 4;
}) {
  const { t } = usePresentation();
  const Heading = headingLevel === 2 ? 'h2' : 'h4';
  return (
    <>
      <Heading>{t('issues.description')}</Heading>
      <p className={styles.prose}>
        {issue.content_available === false
          ? t('issues.contentUnavailable')
          : issue.content.description || t('issues.noDescription')}
      </p>
      <Heading>{t('issues.acceptance')}</Heading>
      <p className={styles.prose}>
        {issue.content_available === false
          ? t('retention.unavailable')
          : issue.content.acceptance || t('issues.noAcceptance')}
      </p>
    </>
  );
}
