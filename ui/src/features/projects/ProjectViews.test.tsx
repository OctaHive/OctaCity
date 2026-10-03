// @vitest-environment jsdom

import { cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import type { ProjectDetails } from './api';
import {
  fakeProjectsApi,
  managementError,
  page,
  project,
  renderProjects,
} from './ProjectTestSupport';

afterEach(cleanup);

describe('Project hierarchy', () => {
  it('loads bounded root pages through the server cursor', async () => {
    const api = fakeProjectsApi();
    api.listProjects
      .mockResolvedValueOnce(page([project('root-a', 'Alpha')], 'next-page'))
      .mockResolvedValueOnce(page([project('root-b', 'Beta')], null));
    renderProjects('/projects', api);

    expect(await screen.findByRole('link', { name: /Alpha/ })).toBeTruthy();
    expect(api.listProjects).toHaveBeenNthCalledWith(1, null, null, expect.any(AbortSignal));

    await userEvent.click(screen.getByRole('button', { name: 'Load more Projects' }));

    expect(await screen.findByRole('link', { name: /Beta/ })).toBeTruthy();
    expect(api.listProjects).toHaveBeenNthCalledWith(2, null, 'next-page', expect.any(AbortSignal));
    expect(screen.queryByRole('button', { name: 'Load more Projects' })).toBeNull();
  });

  it('uses returned ancestry for breadcrumbs and child selection deep links', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({
      ancestors: [project('company', 'Company'), project('platform', 'Platform')],
      project: project('delivery', 'Delivery'),
    });
    api.listProjects.mockResolvedValue(page([project('runtime', 'Runtime', 'delivery')], null));
    const { router } = renderProjects('/projects/delivery', api);

    const breadcrumbs = await screen.findByRole('navigation', { name: 'Project breadcrumb' });
    expect(breadcrumbs.textContent).toContain('ProjectsCompanyPlatformDelivery');
    expect(screen.getByRole('link', { name: 'Company' }).getAttribute('href')).toBe(
      '/projects/company',
    );
    expect(within(breadcrumbs).getByText('Delivery').getAttribute('aria-current')).toBe('page');

    await userEvent.click(await screen.findByRole('link', { name: /Runtime/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/projects/runtime'));
  });

  it('shows an initial loading state', () => {
    const api = fakeProjectsApi();
    api.getProject.mockReturnValue(new Promise<ProjectDetails>(() => undefined));
    renderProjects('/projects/delivery', api);

    expect(screen.getByRole('heading', { name: 'Project' })).toBeTruthy();
    expect(screen.getByText('Loading Projects…').getAttribute('role')).toBe('status');
  });

  it('distinguishes an empty root collection from a request failure', async () => {
    const api = fakeProjectsApi();
    api.listProjects.mockResolvedValue(page([], null));
    renderProjects('/projects', api);

    expect(await screen.findByText('No root Projects are available.')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('renders a not-found deep link without converting it to an empty collection', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockRejectedValue(managementError('not_found', 'request-not-found'));
    renderProjects('/projects/missing', api);

    expect(await screen.findByRole('heading', { name: 'Project not found' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Return to Projects' })).toBeTruthy();
    expect(api.listProjects).not.toHaveBeenCalled();
  });

  it('retains loaded Projects and marks them stale after refresh fails', async () => {
    const api = fakeProjectsApi();
    api.listProjects
      .mockResolvedValueOnce(page([project('root-a', 'Alpha')], null))
      .mockRejectedValueOnce(managementError('unavailable', 'request-refresh'));
    renderProjects('/projects', api);

    expect(await screen.findByRole('link', { name: /Alpha/ })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Refresh' }));

    expect(
      (
        await screen.findByText('Refresh failed. Showing the last loaded Project data.')
      ).getAttribute('role'),
    ).toBe('status');
    expect(screen.getByRole('link', { name: /Alpha/ })).toBeTruthy();
    expect(screen.queryByText('Projects could not be loaded.')).toBeNull();
  });

  it('keeps safe request correlation on a terminal collection failure', async () => {
    const api = fakeProjectsApi();
    api.listProjects.mockRejectedValue(managementError('unavailable', 'request-projects'));
    renderProjects('/projects', api);

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Projects could not be loaded.');
    expect(alert.textContent).toContain('request-projects');
  });
});
