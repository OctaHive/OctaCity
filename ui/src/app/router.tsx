import { Navigate, createBrowserRouter, type RouteObject, useParams } from 'react-router-dom';

import { fetchReadiness, type ReadinessProbe } from '../api/readiness';
import { CONSOLE_PATHS, type ConsolePath } from './routes';
import { AppShell } from './shell/AppShell';
import { NotFoundView, RouteErrorBoundary } from './shell/RouteErrorBoundary';
import styles from './shell/Shell.module.css';

interface PlaceholderViewProps {
  description: string;
  parameter?: 'projectId' | 'buildId' | 'agentId' | 'poolId';
  title: string;
}

function PlaceholderView({ description, parameter, title }: PlaceholderViewProps) {
  const parameters = useParams();
  const identity = parameter === undefined ? undefined : parameters[parameter];

  return (
    <section className={styles.placeholder} aria-labelledby="page-title">
      <p className={styles.eyebrow}>Operator workspace</p>
      <h1 id="page-title">{title}</h1>
      {identity === undefined ? null : <p className={styles.identity}>{identity}</p>}
      <p className={styles.placeholderDescription}>{description}</p>
    </section>
  );
}

/** Returns the complete first-release route tree for browser and memory routers. */
export function createConsoleRoutes(
  readinessProbe: ReadinessProbe = fetchReadiness,
): RouteObject[] {
  return [
    {
      path: '/',
      element: <AppShell readinessProbe={readinessProbe} />,
      errorElement: <RouteErrorBoundary />,
      children: [
        { index: true, element: <Navigate replace to={CONSOLE_PATHS.projects} /> },
        {
          path: childPath(CONSOLE_PATHS.projects),
          element: (
            <PlaceholderView
              description="Browse the bounded Project hierarchy and select an operator workspace."
              title="Projects"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.project),
          element: (
            <PlaceholderView
              description="Current definitions and recent Builds will appear in this Project view."
              parameter="projectId"
              title="Project"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.build),
          element: (
            <PlaceholderView
              description="Attempt, Job, event, output, and retention diagnostics will appear here."
              parameter="buildId"
              title="Build"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.agents),
          element: (
            <PlaceholderView
              description="Inspect bounded Agent capacity and readiness."
              title="Agents"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.agent),
          element: (
            <PlaceholderView
              description="Inventory, assignment, drain, and current execution will appear here."
              parameter="agentId"
              title="Agent"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPools),
          element: (
            <PlaceholderView
              description="Inspect versioned Agent Pool capacity and drain state."
              title="Agent Pools"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.agentPool),
          element: (
            <PlaceholderView
              description="Pool definition, admitted inventory, and capacity will appear here."
              parameter="poolId"
              title="Agent Pool"
            />
          ),
        },
        {
          path: childPath(CONSOLE_PATHS.audit),
          element: (
            <PlaceholderView
              description="Search immutable audit evidence without treating it as authorization state."
              title="Audit"
            />
          ),
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
