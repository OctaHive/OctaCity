import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import type { Issue } from './api';
import type { IssueExecutionsApi } from './executionApi';
/** One exact request identity is shared by Runs and Results, never selected from a latest page. */
export function useIssueExecution(api: IssueExecutionsApi, issue: Issue, requestId: string | null) {
  return useQuery({
    queryKey: queryKeys.issueExecution(issue.project_id, issue.id, requestId ?? ''),
    gcTime: 0,
    enabled: !!requestId,
    queryFn: ({ signal }) => api.get(issue.project_id, issue.id, requestId!, signal),
  });
}
