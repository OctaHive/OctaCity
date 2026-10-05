import { lazy, Suspense, type ReactNode } from 'react';
import { Navigate, createBrowserRouter, type RouteObject } from 'react-router-dom';

import { fetchReadiness, type ReadinessProbe } from '../api/readiness';
import { auditApi, type AuditApi } from '../features/audit/api';
import { buildDiagnosticsApi, type BuildDiagnosticsApi } from '../features/builds/api';
import type { BuildExplorerApi } from '../features/builds/BuildExplorer';
import { capacityApi, type CapacityApi } from '../features/capacity/api';
import { projectsApi, type ProjectsApi } from '../features/projects/api';
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

interface PlaceholderViewProps {
  description: string;
  title: string;
}

function PlaceholderView({ description, title }: PlaceholderViewProps) {
  return (
    <section className={styles.placeholder} aria-labelledby="page-title">
      <p className={styles.eyebrow}>Operator workspace</p>
      <h1 id="page-title">{title}</h1>
      <p className={styles.placeholderDescription}>{description}</p>
    </section>
  );
}

/** Returns the complete first-release route tree for browser and memory routers. */
export function createConsoleRoutes(
  readinessProbe: ReadinessProbe = fetchReadiness,
  projectApi: ProjectsApi = projectsApi,
  buildApi: BuildDiagnosticsApi = buildDiagnosticsApi,
  agentCapacityApi: CapacityApi = capacityApi,
  auditFactsApi: AuditApi = auditApi,
): RouteObject[] {
  const buildExplorerApi: BuildExplorerApi = {
    getBuild: buildApi.getBuild,
    getBuildConfiguration: projectApi.getBuildConfiguration,
    getProject: projectApi.getProject,
    listBuildConfigurations: projectApi.listBuildConfigurations,
    listBuilds: projectApi.listBuilds,
    listProjects: projectApi.listProjects,
  };
  return [
    {
      path: '/',
      element: (
        <AppShell
          explorerContent={{
            audit: defer(<AuditFilterPanel />),
            builds: defer(<BuildExplorer api={buildExplorerApi} />),
            agents: defer(<CapacityExplorer api={agentCapacityApi} />),
            projects: defer(<ProjectExplorer api={projectApi} />),
          }}
          readinessProbe={readinessProbe}
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
          element: defer(<ProjectView api={projectApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.builds),
          element: (
            <PlaceholderView
              description="Choose a Build from the contextual explorer or global search."
              title="Builds"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.build),
          element: defer(<BuildView api={buildApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.agents),
          element: defer(<CapacityLandingView />),
        },
        {
          path: childPath(CONSOLE_PATHS.agent),
          element: defer(<AgentView api={agentCapacityApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPools),
          element: defer(<CapacityLandingView title="Agent Pools" />),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPool),
          element: defer(<AgentPoolView api={agentCapacityApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.audit),
          element: defer(<AuditView api={auditFactsApi} />),
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
  return (
    <Suspense
      fallback={
        <p className={styles.deferred} role="status">
          Loading view…
        </p>
      }
    >
      {view}
    </Suspense>
  );
}
