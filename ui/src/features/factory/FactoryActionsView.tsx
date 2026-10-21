import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import type { FactoryActionsApi } from '../../api/factoryActions';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS } from '../../app/routes';
import { FactoryActions } from '../../shared/FactoryActions';
import type { ProjectHierarchyApi } from '../../shared/projectHierarchy';
import { QueryBackgroundNotice } from '../../shared/QueryStateNotice';
/** Global entry and Project deep links share the card's exact operator-action workspace. */
export function FactoryActionsView({
  api,
  projects,
}: {
  api: FactoryActionsApi;
  projects: Pick<ProjectHierarchyApi, 'getProject'>;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const project = params.get('project_id') ?? undefined;
  const label = useQuery({
    queryKey: queryKeys.project(project ?? ''),
    enabled: project !== undefined,
    queryFn: ({ signal }) => projects.getProject(project!, signal),
  });
  return (
    <section>
      <h1>{t('factoryActions.workspace')}</h1>
      <nav>
        <Link to={CONSOLE_PATHS.factory}>{t('section.factory')}</Link>
        {' · '}
        <Link to={CONSOLE_PATHS.factoryActions}>{t('factoryActions.allProjects')}</Link>
        {project ? (
          <>
            <p>{label.data?.project.name ?? t('projects.project')}</p>
            <QueryBackgroundNotice
              label={t('projects.project')}
              error={label.error}
              fetching={label.isFetching}
              onRetry={label.refetch}
            />
          </>
        ) : null}
      </nav>
      <FactoryActions api={api} scope={{ projectId: project }} />
    </section>
  );
}
