/** Stable browser paths shared by routing, navigation, and route tests. */
export const CONSOLE_PATHS = {
  agent: '/agents/:agentId',
  agentPool: '/agent-pools/:poolId',
  agentPools: '/agent-pools',
  agents: '/agents',
  audit: '/audit',
  build: '/builds/:buildId',
  builds: '/builds',
  project: '/projects/:projectId',
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
