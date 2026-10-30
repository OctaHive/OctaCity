// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it } from 'vitest';
import { runFixture } from '../../tests/fixtures/factory';
import { issueFixture } from '../../tests/fixtures/issues';
import { FactoryRunSummary } from './FactoryRunSummary';

afterEach(cleanup);

it.each([0, 7])(
  'shows unpriced usage without presenting its known subtotal %s as total cost',
  (known) => {
    const resource = runFixture();
    resource.diagnostics.budget.usage.cost_micro_units = {
      status: 'unknown',
      known_micro_units: known,
    };
    render(
      <RouterProvider
        router={createMemoryRouter([{ path: '/', element: <FactoryRunSummary run={resource} /> }])}
      />,
    );
    const metric = screen.getByText('Cost, micro-units').closest('div')!;
    expect(within(metric).getByText('Unknown')).toBeTruthy();
    expect(metric.textContent).not.toContain('0 /');
    if (known > 0) expect(within(metric).getByText(/Known subtotal: 7/)).toBeTruthy();
    else expect(within(metric).queryByText(/Known subtotal/)).toBeNull();
  },
);
it.each([null, 'reviewed-request'])(
  'links an authorized original Run to its exact task lineage (%s)',
  (request) => {
    const run = runFixture();
    const issue = issueFixture(run.project_id);
    const association = {
      project_id: run.project_id,
      issue_id: issue.id,
      work_id: run.work_id,
      run_id: run.id,
      request_id: request,
      requirements_version: request ? 7 : null,
    };
    const resource = { ...run, issue_association: association };
    const router = createMemoryRouter([
      { path: '/', element: <FactoryRunSummary run={resource} /> },
    ]);
    render(<RouterProvider router={router} />);
    const link = screen.getByRole('link', { name: 'Open task' });
    const url = new URL(link.getAttribute('href')!, 'https://console.example');
    expect(url.pathname).toBe(`/projects/${run.project_id}/issues/${issue.id}`);
    expect(url.searchParams.get('section')).toBe('runs');
    expect(url.searchParams.get(request ? 'execution' : 'work')).toBe(request ?? run.work_id);
    expect(url.searchParams.has(request ? 'work' : 'execution')).toBe(false);
  },
);

it('treats an unavailable task link as incomplete coverage without claiming the Run has no task', () => {
  const resource = { ...runFixture(), issue_association: null };
  render(
    <RouterProvider
      router={createMemoryRouter([{ path: '/', element: <FactoryRunSummary run={resource} /> }])}
    />,
  );
  expect(screen.getByText('Task link unavailable or association pending.')).toBeTruthy();
  expect(screen.queryByRole('link', { name: 'Open task' })).toBeNull();
});
