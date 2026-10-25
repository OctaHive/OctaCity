/** Stable browser paths shared by routing, navigation, and route tests. */
export const CONSOLE_PATHS = {
  agent: '/agents/:agentId',
  agentPool: '/agent-pools/:poolId',
  agentPools: '/agent-pools',
  agents: '/agents',
  aiSettings: '/ai-settings',
  audit: '/audit',
  build: '/builds/:buildId',
  builds: '/builds',
  factory: '/factory',
  factoryActions: '/factory-actions',
  factoryRun: '/factory/:factoryRunId',
  integrations: '/integrations',
  integration: '/integrations/:integrationId',
  project: '/projects/:projectId',
  issues: '/projects/:projectId/issues',
  issue: '/projects/:projectId/issues/:issueId',
  issueWorkflows: '/projects/:projectId/issue-workflows',
  projects: '/projects',
} as const;

export type ConsolePath = (typeof CONSOLE_PATHS)[keyof typeof CONSOLE_PATHS];

/** Returns the audit view filtered to one management request identity. */
export function auditRequestPath(requestIdentity: string): string {
  const parameters = new URLSearchParams({ request_identity: requestIdentity });
  return `${CONSOLE_PATHS.audit}?${parameters.toString()}`;
}

/** Returns the stable deep link for one Build identity. */
export function buildPath(buildId: string): string {
  return `/builds/${encodeURIComponent(buildId)}`;
}

/** Returns the stable deep link for one Agent identity. */
export function agentPath(agentId: string): string {
  return `/agents/${encodeURIComponent(agentId)}`;
}

/** Returns the stable deep link for one Agent Pool identity. */
export function agentPoolPath(poolId: string): string {
  return `/agent-pools/${encodeURIComponent(poolId)}`;
}

/** Returns the stable deep link for one Project identity. */
export function projectPath(projectId: string): string {
  return `/projects/${encodeURIComponent(projectId)}`;
}

/** Returns the canonical detail route for a globally searchable resource. */
export function searchableResourcePath(
  kind: 'agent' | 'agent_pool' | 'build' | 'project',
  id: string,
): string {
  switch (kind) {
    case 'agent':
      return agentPath(id);
    case 'agent_pool':
      return agentPoolPath(id);
    case 'build':
      return buildPath(id);
    case 'project':
      return projectPath(id);
  }
}

/** Project-owned business workflow settings. */
export function issueWorkflowsPath(projectId: string): string {
  return `${projectPath(projectId)}/issue-workflows`;
}

export function issuesPath(projectId: string): string {
  return `${projectPath(projectId)}/issues`;
}
export function issuePath(projectId: string, issueId: string): string {
  return `${issuesPath(projectId)}/${encodeURIComponent(issueId)}`;
}

/** AI-agent and Runtime setup keep only Project identity and the selected view in the URL. */
export function aiSettingsPath(projectId: string, view: 'agents' | 'runtimes' = 'agents'): string {
  return `${CONSOLE_PATHS.aiSettings}?${new URLSearchParams({ project_id: projectId, view })}`;
}
