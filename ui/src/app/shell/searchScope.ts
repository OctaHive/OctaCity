import type { ResourceSearchKind } from '../../api/resourceSearch';
import type { MessageKey } from '../presentation/messages';
import type { ConsoleSectionId } from './sections';

export type ResourceSearchScope = readonly ResourceSearchKind[];

export const GLOBAL_RESOURCE_SEARCH_SCOPE: ResourceSearchScope = [];

export const resourceKindMessageKeys = {
  agent: 'command.kind.agent',
  agent_pool: 'command.kind.agent_pool',
  build: 'command.kind.build',
  project: 'command.kind.project',
} as const satisfies Record<ResourceSearchKind, MessageKey>;

const sectionScopes = {
  agents: ['agent', 'agent_pool'],
  audit: [],
  builds: ['build'],
  projects: ['project'],
} as const satisfies Record<ConsoleSectionId, ResourceSearchScope>;

/** Returns the server resource kinds selected by one explorer search entry point. */
export function searchScopeForSection(sectionId: ConsoleSectionId): ResourceSearchScope {
  return sectionScopes[sectionId];
}
