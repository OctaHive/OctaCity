/** Stable browser paths shared by routing, navigation, and route tests. */
export const CONSOLE_PATHS = {
  agent: '/agents/:agentId',
  agentPool: '/agent-pools/:poolId',
  agentPools: '/agent-pools',
  agents: '/agents',
  audit: '/audit',
  build: '/builds/:buildId',
  project: '/projects/:projectId',
  projects: '/projects',
} as const;

export type ConsolePath = (typeof CONSOLE_PATHS)[keyof typeof CONSOLE_PATHS];

/** Returns the stable deep link for one Project identity. */
export function projectPath(projectId: string): string {
  return `/projects/${encodeURIComponent(projectId)}`;
}
