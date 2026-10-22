import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { requireManagementResponse } from '../../api/client';
import type { Issue } from './api';
import type { IssueExecutionsApi, IssueLegacyWorkApi } from './executionApi';
/** One exact request identity is shared by Runs and Results, never selected from a latest page. */
export function useIssueExecution(api: IssueExecutionsApi, issue: Issue, requestId: string | null) {
  return useQuery({
    queryKey: queryKeys.issueExecution(issue.project_id, issue.id, requestId ?? ''),
    gcTime: 0,
    enabled: !!requestId,
    queryFn: ({ signal }) => api.get(issue.project_id, issue.id, requestId!, signal),
  });
}

/** Requestless Work keeps its original association in both Runs and Results. */
export function useIssueWork(api: IssueLegacyWorkApi, issue: Issue, work: string | null) {
  return useQuery({
    queryKey: queryKeys.issueLegacyWork(issue.project_id, issue.id, work),
    enabled: work !== null,
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const data = await api.getWork(issue.project_id, issue.id, work!, signal);
      const a = data.association;
      return requireManagementResponse(
        a.project_id === issue.project_id && a.issue_id === issue.id && a.work_id === work
          ? data
          : undefined,
      );
    },
  });
}
