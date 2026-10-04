import { CONSOLE_PATHS } from '../routes';

export type ConsoleSectionId = 'projects' | 'builds' | 'agents' | 'audit';

export interface ConsoleSection {
  id: ConsoleSectionId;
  label: string;
  path: string;
}

const projectsSection: ConsoleSection = {
  id: 'projects',
  label: 'Projects',
  path: CONSOLE_PATHS.projects,
};

export const consoleSections: readonly ConsoleSection[] = [
  projectsSection,
  { id: 'builds', label: 'Builds', path: CONSOLE_PATHS.builds },
  { id: 'agents', label: 'Agents', path: CONSOLE_PATHS.agents },
  { id: 'audit', label: 'Audit', path: CONSOLE_PATHS.audit },
];

/** Maps every stable resource deep link to its contextual workbench section. */
export function sectionForPath(pathname: string): ConsoleSection {
  const sectionId: ConsoleSectionId = matchesPathSegment(pathname, CONSOLE_PATHS.builds)
    ? 'builds'
    : matchesPathSegment(pathname, CONSOLE_PATHS.agents) ||
        matchesPathSegment(pathname, CONSOLE_PATHS.agentPools)
      ? 'agents'
      : matchesPathSegment(pathname, CONSOLE_PATHS.audit)
        ? 'audit'
        : 'projects';
  return consoleSections.find(({ id }) => id === sectionId) ?? projectsSection;
}

function matchesPathSegment(pathname: string, basePath: string): boolean {
  return pathname === basePath || pathname.startsWith(`${basePath}/`);
}
