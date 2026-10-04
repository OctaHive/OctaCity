import type { components } from '../../../.generated/api/schema';

import type { ConsoleSectionId } from './sections';

export type ResourceSearchKind = components['schemas']['ResourceSearchKind'];
export type ResourceSearchScope = readonly ResourceSearchKind[];

export const GLOBAL_RESOURCE_SEARCH_SCOPE: ResourceSearchScope = [];

const sectionScopes = {
  agents: ['agent', 'agent_pool'],
  audit: [],
  builds: ['build'],
  projects: ['project'],
} as const satisfies Record<ConsoleSectionId, ResourceSearchScope>;

const kindLabels = {
  agent: 'Agents',
  agent_pool: 'Agent Pools',
  build: 'Builds',
  project: 'Projects',
} as const satisfies Record<ResourceSearchKind, string>;

/** Returns the server resource kinds selected by one explorer search entry point. */
export function searchScopeForSection(sectionId: ConsoleSectionId): ResourceSearchScope {
  return sectionScopes[sectionId];
}

export function searchScopeLabel(scope: ResourceSearchScope): string {
  return scope.length === 0 ? 'All resources' : scope.map((kind) => kindLabels[kind]).join(' and ');
}
