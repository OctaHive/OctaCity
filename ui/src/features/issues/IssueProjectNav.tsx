import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { projectPath, issuesPath } from '../../app/routes';
import type { ProjectHierarchyApi } from '../../shared/projectHierarchy';
import styles from './Issues.module.css';

export type IssueProjectReader = Pick<ProjectHierarchyApi, 'getProject'>;
/** Project context stays visible when the task is opened from a direct URL. */
export function IssueProjectNav({
  projects,
  projectId,
  card = false,
}: {
  projects: IssueProjectReader;
  projectId: string;
  card?: boolean;
}) {
  const { t } = usePresentation();
  const project = useQuery({
    queryKey: queryKeys.project(projectId),
    queryFn: ({ signal }) => projects.getProject(projectId, signal),
  });
  return (
    <nav aria-label={t('issues.breadcrumb')} className={styles.actions}>
      <Link to={projectPath(projectId)}>{project.data?.project.name ?? t('projects.project')}</Link>
      {card ? <Link to={issuesPath(projectId)}>{t('issues.tasks')}</Link> : null}
    </nav>
  );
}
