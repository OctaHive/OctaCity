import { createFactoryActionsApi, type FactoryActionsApi } from '../api/factoryActions';
import {
  issueResultPanelsApi,
  type IssueResultPanelsApi,
} from '../features/issues/resultPanelsApi';
import {
  issueCodeReviewsApi,
  type IssueCodeReviewsApi,
  type IssueReviewConnections,
} from '../features/issues/codeReviewApi';
import { lazy, Suspense, type ReactNode } from 'react';
import { Navigate, createBrowserRouter, type RouteObject } from 'react-router-dom';

import { fetchReadiness, type ReadinessProbe } from '../api/readiness';
import { resourceSearchApi, type ResourceSearchApi } from '../api/resourceSearch';
import { operatorAttentionApi, type OperatorAttentionApi } from '../api/operatorAttention';
import { auditApi, type AuditApi } from '../features/audit/api';
import { factoryApi, type FactoryApi } from '../features/factory/api';
import { integrationsApi, type IntegrationsApi } from '../features/integrations/api';
import { buildDiagnosticsApi, type BuildDiagnosticsApi } from '../features/builds/api';
import type { BuildExplorerApi } from '../features/builds/BuildExplorer';
import { capacityApi, type CapacityApi } from '../features/capacity/api';
import { issueActivityApi, type IssueActivityApi } from '../features/issues/activityApi';
import {
  issueExecutionsApi,
  type IssueExecutionsApi,
  type IssueLegacyWorkApi,
} from '../features/issues/executionApi';
import { issuesApi, type IssuesApi } from '../features/issues/api';
import { issueSourcesApi, type IssueSourcesApi } from '../features/issues/sourceApi';
import { issueWorkflowsApi, type IssueWorkflowsApi } from '../api/issueWorkflows';
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
const IntegrationsView = lazy(async () => ({
  default: (await import('../features/integrations/views')).IntegrationsView,
}));
const IntegrationDetailView = lazy(async () => ({
  default: (await import('../features/integrations/views')).IntegrationDetailView,
}));
const FactoryExplorer = lazy(async () => ({
  default: (await import('../features/factory/FactoryExplorer')).FactoryExplorer,
}));
const FactoryLandingView = lazy(async () => ({
  default: (await import('../features/factory/FactoryViews')).FactoryLandingView,
}));
const FactoryRunView = lazy(async () => ({
  default: (await import('../features/factory/FactoryViews')).FactoryRunView,
}));
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
const IssuesView = lazy(async () => ({
  default: (await import('../features/issues/IssuesView')).IssuesView,
}));
const FactoryActionsView = lazy(async () => ({
  default: (await import('../features/factory/FactoryActionsView')).FactoryActionsView,
}));
const IssueView = lazy(async () => ({
  default: (await import('../features/issues/IssueView')).IssueView,
}));
const WorkflowView = lazy(async () => ({
  default: (await import('../features/issueWorkflows/WorkflowView')).WorkflowView,
}));
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
  readonly integrationsApi?: IntegrationsApi;
  readonly factoryApi: FactoryApi;
  readonly factoryActionsApi: FactoryActionsApi;
  readonly attentionApi: OperatorAttentionApi;
  readonly auditApi: AuditApi;
  readonly buildApi: BuildDiagnosticsApi;
  readonly capacityApi: CapacityApi;
  readonly issueResultPanelsApi: IssueResultPanelsApi;
  readonly issueActivityApi: IssueActivityApi;
  readonly issueExecutionsApi: IssueExecutionsApi;
  readonly issueLegacyWorkApi: IssueLegacyWorkApi;
  readonly issuesApi: IssuesApi;
  readonly issueSourcesApi: IssueSourcesApi;
  readonly issueCodeReviewsApi: IssueCodeReviewsApi;
  readonly issueReviewConnections: IssueReviewConnections;
  readonly issueWorkflowsApi: IssueWorkflowsApi;
  readonly projectsApi: ProjectsApi;
  readonly readinessProbe: ReadinessProbe;
  readonly searchApi: ResourceSearchApi;
}

const DEFAULT_ROUTE_DEPENDENCIES: ConsoleRouteDependencies = {
  issueActivityApi,
  issueResultPanelsApi,
  factoryApi,
  factoryActionsApi: createFactoryActionsApi(),
  attentionApi: operatorAttentionApi,
  auditApi,
  buildApi: buildDiagnosticsApi,
  capacityApi,
  projectsApi,
  issueWorkflowsApi,
  issuesApi,
  issueSourcesApi,
  issueCodeReviewsApi,
  issueReviewConnections: integrationsApi,
  issueExecutionsApi,
  issueLegacyWorkApi: issueExecutionsApi,
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
            factory: defer(
              <FactoryExplorer api={dependencies.factoryApi} projects={dependencies.projectsApi} />,
            ),
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
        {
          path: childPath(CONSOLE_PATHS.integrations),
          element: defer(<IntegrationsView api={dependencies.integrationsApi} />),
        },
        {
          path: childPath(CONSOLE_PATHS.integration),
          element: defer(
            <IntegrationDetailView
              api={dependencies.integrationsApi}
              projects={dependencies.projectsApi}
            />,
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.factory),
          element: defer(
            <FactoryLandingView
              workflows={dependencies.issueWorkflowsApi}
              api={dependencies.factoryApi}
              workSources={{
                getProject: dependencies.projectsApi.getProject,
                getRepository: dependencies.projectsApi.getRepository,
                listRepositories: dependencies.projectsApi.listRepositories,
                listBuilds: dependencies.projectsApi.listBuilds,
                listBuildArtifacts: dependencies.buildApi.listBuildArtifacts,
              }}
            />,
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.factoryActions),
          element: defer(
            <FactoryActionsView
              api={dependencies.factoryActionsApi}
              projects={dependencies.projectsApi}
            />,
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.factoryRun),
          element: defer(<FactoryRunView api={dependencies.factoryApi} />),
        },
        { index: true, element: <Navigate replace to={CONSOLE_PATHS.projects} /> },
        {
          path: childPath(CONSOLE_PATHS.projects),
          element: defer(<ProjectsLandingView />),
        },
        {
          path: childPath(CONSOLE_PATHS.issues),
          element: defer(
            <IssuesView api={dependencies.issuesApi} projects={dependencies.projectsApi} />,
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.issue),
          element: defer(
            <IssueView
              factory={dependencies.factoryApi}
              actions={dependencies.factoryActionsApi}
              panels={dependencies.issueResultPanelsApi}
              activity={dependencies.issueActivityApi}
              sources={dependencies.issueSourcesApi}
              reviews={dependencies.issueCodeReviewsApi}
              reviewConnections={dependencies.issueReviewConnections}
              executions={dependencies.issueExecutionsApi}
              legacy={dependencies.issueLegacyWorkApi}
              executionSources={{
                listConfigurations: dependencies.factoryApi.listConfigurations,
                listRepositories: dependencies.projectsApi.listRepositories,
                getRepository: dependencies.projectsApi.getRepository,
              }}
              api={dependencies.issuesApi}
              workflows={dependencies.issueWorkflowsApi}
              projects={dependencies.projectsApi}
            />,
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.issueWorkflows),
          element: defer(<WorkflowView api={dependencies.issueWorkflowsApi} />),
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
