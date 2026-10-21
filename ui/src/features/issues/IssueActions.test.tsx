// @vitest-environment jsdom
import { cleanup, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
import type { FactoryActionsApi } from '../../api/factoryActions';
afterEach(cleanup);
it.each([
  { path: '/factory-actions', scope: {} },
  { path: '/factory-actions?project_id=delivery', scope: { projectId: 'delivery' } },
  {
    path: '/projects/delivery/issues/' + issue.id + '?section=actions',
    scope: { projectId: 'delivery', issueId: issue.id },
  },
])('opens the same bounded action view for $path', async ({ path, scope }) => {
  const api = {
    list: vi.fn<FactoryActionsApi['list']>().mockResolvedValue({
      items: [],
      next_cursor: null,
      total: 0,
      coverage: 'complete',
      observed_at: 12,
    }),
    observe: vi.fn<FactoryActionsApi['observe']>(),
    respondNode: vi.fn<FactoryActionsApi['respondNode']>(),
    requestNodeOperation: vi.fn<FactoryActionsApi['requestNodeOperation']>(),
    retryNode: vi.fn<FactoryActionsApi['retryNode']>(),
  };
  setup(path, undefined, { factoryActionsApi: api });
  expect(await screen.findByRole('heading', { name: 'Execution actions' })).toBeTruthy();
  await waitFor(() => expect(api.list).toHaveBeenCalledWith(scope, null, expect.any(AbortSignal)));
  expect(await screen.findByText('No unanswered actions.')).toBeTruthy();
  expect(api.respondNode).not.toHaveBeenCalled();
});
