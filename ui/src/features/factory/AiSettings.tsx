import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { projectPath } from '../../app/routes';
import type { ProjectHierarchyApi } from '../../shared/projectHierarchy';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { AiAgents } from './AiAgents';
import { readFactoryProjectId } from './filters';
import { catalogLabels } from './catalogLabels';
import { factoryRolesApi, type FactoryRolesApi } from './roleApi';
import type { RuntimeIntegrationApi } from './RuntimeIntegrations';
import { RuntimeSettings, type RuntimeApi } from './RuntimeSettings';
import { factoryRuntimeApi } from './runtimeApi';
import styles from './Factory.module.css';

/** Project-owned AI authoring has one home, separate from enrolled worker management. */
export function AiSettings({
  projects,
  roles = factoryRolesApi,
  runtime = factoryRuntimeApi,
  integrations,
}: {
  projects: ProjectHierarchyApi;
  roles?: FactoryRolesApi;
  runtime?: RuntimeApi;
  integrations: RuntimeIntegrationApi;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const projectId = readFactoryProjectId(params);
  const runtimes = params.get('view') === 'runtimes';
  const client = useQueryClient();
  const previousViewRef = useRef(runtimes);
  useEffect(() => {
    // Re-read choices on return; mounted profile state remains the operator's intent.
    if (previousViewRef.current && !runtimes && projectId) {
      void client.invalidateQueries({
        queryKey: queryKeys.factoryRoleCatalog(projectId, 'capabilities'),
      });
      void client.invalidateQueries({
        queryKey: queryKeys.factoryRoleCatalog(projectId, 'capability').slice(0, 4),
      });
    }
    previousViewRef.current = runtimes;
  }, [client, projectId, runtimes]);
  const [parent, setParent] = useState<string | null>(null);
  const choices = useCursorPage(queryKeys.projectChildren(parent), (cursor, signal) =>
    projects.listProjects(parent, cursor, signal),
  );
  const project = useQuery({
    queryKey: queryKeys.project(projectId ?? ''),
    queryFn: ({ signal }) => projects.getProject(projectId!, signal),
    enabled: projectId !== null,
  });
  const select = (id: string) => {
    const next = new URLSearchParams(params);
    next.set('project_id', id);
    setParams(next);
  };
  return (
    <section className={styles.workspace}>
      <h1>{t('section.aiSettings')}</h1>
      <nav aria-label={t('section.aiSettings')}>
        {(['agents', 'runtimes'] as const).map((view) => (
          <button
            key={view}
            type="button"
            aria-pressed={runtimes === (view === 'runtimes')}
            onClick={() => {
              const next = new URLSearchParams(params);
              next.set('view', view);
              setParams(next);
            }}
          >
            {t(view === 'agents' ? 'factory.agents' : 'runtime.runtimes')}
          </button>
        ))}
      </nav>
      {!projectId ? <p>{t('runtime.chooseProject')}</p> : null}
      <details open={!projectId}>
        <summary>
          {t('integrations.project')}
          {project.data ? ` · ${project.data.project.name}` : ''}
        </summary>
        {parent ? (
          <button type="button" onClick={() => setParent(null)}>
            {t('integrations.rootProjects')}
          </button>
        ) : null}
        <PagedExplorerBranch
          query={choices}
          empty={t('explorer.noProjectBranches')}
          labels={catalogLabels(t('integrations.project'))}
        >
          {(items) => (
            <ul>
              {items.map((item) => (
                <li key={item.id}>
                  <button type="button" onClick={() => select(item.id)}>
                    {item.name}
                  </button>
                  {item.has_children ? (
                    <button
                      type="button"
                      aria-label={`${t('integrations.childProjects')} · ${item.name}`}
                      onClick={() => setParent(item.id)}
                    >
                      {t('integrations.childProjects')}
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </PagedExplorerBranch>
      </details>
      {projectId && project.isPending ? (
        <QueryLoadingNotice label={t('integrations.project')} />
      ) : null}
      {projectId && project.error ? (
        <QueryFailureNotice
          title={t('integrations.project')}
          error={project.error}
          onRetry={project.refetch}
        />
      ) : null}
      {project.data && projectId ? (
        <div key={projectId}>
          <h2>
            <Link to={projectPath(projectId)}>{project.data.project.name}</Link>
          </h2>
          <div hidden={runtimes}>
            <AiAgents api={roles} projectId={projectId} />
          </div>
          {runtimes ? (
            <RuntimeSettings project={projectId} api={runtime} integrations={integrations} />
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
