import {
  CONSOLE_PATHS,
  agentPath,
  agentPoolPath,
  buildPath,
  projectPath,
  type ConsolePath,
} from '../../src/app/routes';
import { FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
import { integrationFixture } from '../fixtures/integrations';

export interface PrimaryRoute {
  heading: string;
  id: string;
  path: string;
  retainedText: string;
}

const routesByTemplate = {
  [CONSOLE_PATHS.integrations]: {
    heading: 'Integrations',
    id: 'integrations',
    path: CONSOLE_PATHS.integrations,
    retainedText: 'Tracker',
  },
  [CONSOLE_PATHS.integration]: {
    heading: 'Tracker',
    id: 'integration',
    path: `/integrations/${integrationFixture('IntegrationConnection').id}`,
    retainedText: 'Tracker',
  },
  [CONSOLE_PATHS.factory]: {
    heading: 'Factory',
    id: 'factory',
    path: CONSOLE_PATHS.factory,
    retainedText: 'Requested change',
  },
  [CONSOLE_PATHS.factoryRun]: {
    heading: 'Factory Run',
    id: 'factory-run',
    path: `/factory/${FIXTURE_IDS.factoryRun}`,
    retainedText: 'Requested change',
  },
  [CONSOLE_PATHS.projects]: {
    heading: 'Projects',
    id: 'projects',
    path: CONSOLE_PATHS.projects,
    retainedText: 'Accessible Project',
  },
  [CONSOLE_PATHS.issues]: {
    heading: 'Tasks',
    id: 'issues',
    path: `${projectPath(FIXTURE_IDS.project)}/issues`,
    retainedText: 'Investigate checkout',
  },
  [CONSOLE_PATHS.issue]: {
    heading: 'ISS-42 · Investigate checkout',
    id: 'issue',
    path: `${projectPath(FIXTURE_IDS.project)}/issues/${issueFixture().id}`,
    retainedText: 'Investigate checkout',
  },
  [CONSOLE_PATHS.issueWorkflows]: {
    heading: 'Task workflow',
    id: 'issue-workflows',
    path: `${projectPath(FIXTURE_IDS.project)}/issue-workflows`,
    retainedText: 'Local workflow',
  },
  [CONSOLE_PATHS.project]: {
    heading: 'Accessible Project',
    id: 'project',
    path: projectPath(FIXTURE_IDS.project),
    retainedText: 'Accessible Project',
  },
  [CONSOLE_PATHS.builds]: {
    heading: 'Builds',
    id: 'builds',
    path: CONSOLE_PATHS.builds,
    retainedText: 'Accessible Project',
  },
  [CONSOLE_PATHS.build]: {
    heading: `Build ${FIXTURE_IDS.build}`,
    id: 'build',
    path: buildPath(FIXTURE_IDS.build),
    retainedText: `Build ${FIXTURE_IDS.build}`,
  },
  [CONSOLE_PATHS.agents]: {
    heading: 'Agents',
    id: 'agents',
    path: CONSOLE_PATHS.agents,
    retainedText: 'Accessible Pool',
  },
  [CONSOLE_PATHS.agent]: {
    heading: 'Accessible Agent',
    id: 'agent',
    path: agentPath(FIXTURE_IDS.agent),
    retainedText: 'Accessible Agent',
  },
  [CONSOLE_PATHS.agentPools]: {
    heading: 'Agent Pools',
    id: 'agent-pools',
    path: CONSOLE_PATHS.agentPools,
    retainedText: 'Accessible Pool',
  },
  [CONSOLE_PATHS.agentPool]: {
    heading: 'Accessible Pool',
    id: 'agent-pool',
    path: agentPoolPath(FIXTURE_IDS.pool),
    retainedText: 'Accessible Pool',
  },
  [CONSOLE_PATHS.audit]: {
    heading: 'Audit',
    id: 'audit',
    path: CONSOLE_PATHS.audit,
    retainedText: 'complete-job',
  },
} as const satisfies Record<ConsolePath, PrimaryRoute>;

/** Concrete deterministic cases for every declared primary console route. */
export const PRIMARY_ROUTES = Object.values(routesByTemplate);

export type PrimaryRouteId = (typeof PRIMARY_ROUTES)[number]['id'];

/** Resolves one compile-time inventoried route for focused browser scenarios. */
export function primaryRoute(id: PrimaryRouteId): PrimaryRoute {
  const route = PRIMARY_ROUTES.find((candidate) => candidate.id === id);
  if (route === undefined) throw new Error(`Primary route ${id} is not inventoried`);
  return route;
}
