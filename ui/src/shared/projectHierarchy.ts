import type { components } from '../../.generated/api/schema';

export type ProjectDetails = components['schemas']['ProjectDetails'];
export type ProjectPage = components['schemas']['ProjectPage'];
export type ProjectResource = components['schemas']['ProjectResource'];
export type ProjectSummary = components['schemas']['ProjectSummaryResource'];

/** Navigation read contract shared by activities, independent of Project definition authoring. */
export interface ProjectHierarchyApi {
  getProject(projectId: string, signal?: AbortSignal): Promise<ProjectDetails>;
  listProjects(
    parentId: string | null,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<ProjectPage>;
}
