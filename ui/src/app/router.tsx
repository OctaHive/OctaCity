import { lazy, Suspense, type ReactNode } from 'react';
import { Navigate, createBrowserRouter, type RouteObject } from 'react-router-dom';

import { fetchReadiness, type ReadinessProbe } from '../api/readiness';
import { resourceSearchApi, type ResourceSearchApi } from '../api/resourceSearch';
import { operatorAttentionApi, type OperatorAttentionApi } from '../api/operatorAttention';
import { auditApi, type AuditApi } from '../features/audit/api';
import { buildDiagnosticsApi, type BuildDiagnosticsApi } from '../features/builds/api';
import type { BuildExplorerApi } from '../features/builds/BuildExplorer';
import { capacityApi, type CapacityApi } from '../features/capacity/api';
import { projectsApi, type ProjectsApi } from '../features/projects/api';
import { WorkspaceWelcome, type WorkspaceWelcomeKind } from '../shared/WorkspaceWelcome';
import { usePresentation } from './presentation/PresentationProvider';
import type { MessageKey } from './presentation/messages';
import { CONSOLE_PATHS, type ConsolePath } from './routes';
import { AppShell } from './shell/AppShell';
import { NotFoundView, RouteErrorBoundary } from './shell/RouteErrorBoundary';
import styles from './shell/RouteView.module.css';

const AuditView = lazy(async () => {
  const module = await import('../features/audit/AuditView');
  return { default: module.AuditView };
});
const AuditFilterPanel = lazy(async () => {
  const module = await import('../features/audit/AuditView');
  return { default: module.AuditFilterPanel };
});
const BuildExplorer = lazy(async () => {
  const module = await import('../features/builds/BuildExplorer');
  return { default: module.BuildExplorer };
});
const BuildView = lazy(async () => {
  const module = await import('../features/builds/BuildView');
  return { default: module.BuildView };
});
const CapacityExplorer = lazy(async () => {
  const module = await import('../features/capacity/CapacityExplorer');
  return { default: module.CapacityExplorer };
});
const AgentPoolView = lazy(async () => {
  const module = await import('../features/capacity/CapacityViews');
  return { default: module.AgentPoolView };
});
const AgentView = lazy(async () => {
  const module = await import('../features/capacity/CapacityViews');
  return { default: module.AgentView };
});
const CapacityLandingView = lazy(async () => {
  const module = await import('../features/capacity/CapacityViews');
  return { default: module.CapacityLandingView };
});
const ProjectExplorer = lazy(async () => {
  const module = await import('../features/projects/ProjectExplorer');
  return { default: module.ProjectExplorer };
});
const ProjectsLandingView = lazy(async () => {
  const module = await import('../features/projects/ProjectViews');
  return { default: module.ProjectsLandingView };
});
const ProjectView = lazy(async () => {
  const module = await import('../features/projects/ProjectViews');
  return { default: module.ProjectView };
});

interface WorkspaceLandingViewProps {
  description: MessageKey;
  illustrationLabel: MessageKey;
  kind: WorkspaceWelcomeKind;
  title: MessageKey;
}

function WorkspaceLandingView({
  description,
  illustrationLabel,
  kind,
  title,
}: WorkspaceLandingViewProps) {
  const { t } = usePresentation();
  return (
    <WorkspaceWelcome
      description={t(description)}
      eyebrow={t('route.operatorWorkspace')}
      illustrationLabel={t(illustrationLabel)}
      kind={kind}
      title={t(title)}
    />
  );
}

export interface ConsoleRouteDependencies {
  readonly attentionApi: OperatorAttentionApi;
  readonly auditApi: AuditApi;
  readonly buildApi: BuildDiagnosticsApi;
  readonly capacityApi: CapacityApi;
  readonly projectsApi: ProjectsApi;
  readonly readinessProbe: ReadinessProbe;
  readonly searchApi: ResourceSearchApi;
}

const DEFAULT_ROUTE_DEPENDENCIES: ConsoleRouteDependencies = {
  attentionApi: operatorAttentionApi,
  auditApi,
  buildApi: buildDiagnosticsApi,
  capacityApi,
  projectsApi,
  readinessProbe: fetchReadiness,
  searchApi: resourceSearchApi,
};

/** Returns the complete first-release route tree with named, independently replaceable boundaries. */
export function createConsoleRoutes(
  overrides: Partial<ConsoleRouteDependencies> = {},
): RouteObject[] {
  const dependencies = { ...DEFAULT_ROUTE_DEPENDENCIES, ...overrides };
  const buildExplorerApi: BuildExplorerApi = {
    getBuild: dependencies.buildApi.getBuild,
    getBuildConfiguration: dependencies.projectsApi.getBuildConfiguration,
    getProject: dependencies.projectsApi.getProject,
    listBuildConfigurations: dependencies.projectsApi.listBuildConfigurations,
    listBuilds: dependencies.projectsApi.listBuilds,
    listProjects: dependencies.projectsApi.listProjects,
  };
  return [
    {
      path: '/',
      element: (
        <AppShell
          explorerContent={{
            audit: defer(<AuditFilterPanel />),
            builds: defer(<BuildExplorer api={buildExplorerApi} />),
            agents: defer(<CapacityExplorer api={dependencies.capacityApi} />),
            projects: defer(<ProjectExplorer api={dependencies.projectsApi} />),
          }}
          readinessProbe={dependencies.readinessProbe}
          operatorAttentionApi={dependencies.attentionApi}
          resourceSearchApi={dependencies.searchApi}
        />
      ),
      errorElement: <RouteErrorBoundary />,
      children: [
        { index: true, element: <Navigate replace to={CONSOLE_PATHS.projects} /> },
        {
          path: childPath(CONSOLE_PATHS.projects),
          element: defer(<ProjectsLandingView />),
        },
        {
          path: childPath(CONSOLE_PATHS.project),
          element: defer(<ProjectView api={dependencies.projectsApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.builds),
          element: (
            <WorkspaceLandingView
              description="route.chooseBuild"
              illustrationLabel="workspace.buildsIllustration"
              kind="builds"
              title="section.builds"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.build),
          element: defer(<BuildView api={dependencies.buildApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.agents),
          element: defer(<CapacityLandingView />),
        },
        {
          path: childPath(CONSOLE_PATHS.agent),
          element: defer(<AgentView api={dependencies.capacityApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPools),
          element: defer(<CapacityLandingView kind="pools" />),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPool),
          element: defer(<AgentPoolView api={dependencies.capacityApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.audit),
          element: defer(<AuditView api={dependencies.auditApi} />),
        },
        { path: '*', element: <NotFoundView /> },
      ],
    },
  ];
}

export function createConsoleBrowserRouter() {
  return createBrowserRouter(createConsoleRoutes());
}

function childPath(path: ConsolePath): string {
  return path.slice(1);
}

function defer(view: ReactNode): ReactNode {
  return <DeferredView>{view}</DeferredView>;
}

function DeferredView({ children }: { children: ReactNode }) {
  const { t } = usePresentation();
  return (
    <Suspense
      fallback={
        <p className={styles.deferred} role="status">
          {t('route.loadingView')}
        </p>
      }
    >
      {children}
    </Suspense>
  );
}
