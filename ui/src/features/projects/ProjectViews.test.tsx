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

    const explorer = screen.getByRole('complementary', { name: 'Projects explorer' });
    expect(await within(explorer).findByRole('link', { name: 'Alpha' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenNthCalledWith(1, null, null, expect.any(AbortSignal));

    await userEvent.click(screen.getByRole('button', { name: 'Load more root Projects' }));

    expect(await within(explorer).findByRole('link', { name: 'Beta' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenNthCalledWith(2, null, 'next-page', expect.any(AbortSignal));
    expect(screen.queryByRole('button', { name: 'Load more root Projects' })).toBeNull();
    expect(within(screen.getByRole('main')).queryByRole('link', { name: 'Alpha' })).toBeNull();
    expect(screen.getByText('Select a Project from the hierarchy on the left.')).toBeTruthy();
  });

  it('uses returned ancestry for breadcrumbs and child selection deep links', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({
      ancestors: [project('company', 'Company'), project('platform', 'Platform')],
      project: project('delivery', 'Delivery'),
    });
    api.listProjects.mockImplementation(async (parentId) => {
      switch (parentId) {
        case null:
          return page([project('alpha', 'Alpha')], 'more-roots');
        case 'company':
          return page([], null);
        case 'platform':
          return page([], null);
        case 'delivery':
          return page([project('runtime', 'Runtime', 'delivery')], null);
        default:
          return page([], null);
      }
    });
    const { router } = renderProjects('/projects/delivery', api);

    const breadcrumbs = await screen.findByRole('navigation', { name: 'Project breadcrumb' });
    expect(breadcrumbs.textContent).toContain('ProjectsCompanyPlatformDelivery');
    expect(screen.getByRole('link', { name: 'Company' }).getAttribute('href')).toBe(
      '/projects/company',
    );
    expect(within(breadcrumbs).getByText('Delivery').getAttribute('aria-current')).toBe('page');

    const explorer = screen.getByRole('complementary', { name: 'Projects explorer' });
    expect(
      (await within(explorer).findByRole('link', { name: 'Delivery' })).getAttribute(
        'aria-current',
      ),
    ).toBe('page');
    await userEvent.click(await within(explorer).findByRole('link', { name: 'Runtime' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/projects/runtime'));
  });

  it('expands and paginates sibling branches independently in the explorer', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({
      ancestors: [project('alpha', 'Alpha')],
      project: project('alpha-one', 'Alpha one', 'alpha'),
    });
    api.listProjects.mockImplementation(async (parentId, cursor) => {
      if (parentId === null) {
        return page([project('alpha', 'Alpha'), project('beta', 'Beta')], null);
      }
      if (parentId === 'alpha' && cursor === null) {
        return page([project('alpha-one', 'Alpha one', 'alpha')], 'alpha-next');
      }
      if (parentId === 'alpha' && cursor === 'alpha-next') {
        return page([project('alpha-two', 'Alpha two', 'alpha')], null);
      }
      if (parentId === 'beta') {
        return page([project('beta-one', 'Beta one', 'beta')], null);
      }
      return page([], null);
    });
    const { router } = renderProjects('/projects', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Alpha' }));
    expect(await screen.findByRole('link', { name: 'Alpha one' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Expand Beta' }));
    expect(await screen.findByRole('link', { name: 'Beta one' })).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Load more children of Alpha' }));
    expect(await screen.findByRole('link', { name: 'Alpha two' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenCalledWith('alpha', 'alpha-next', expect.any(AbortSignal));

    await userEvent.click(screen.getByRole('link', { name: 'Alpha one' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/projects/alpha-one'));
    expect(screen.getByRole('link', { name: 'Beta one' })).toBeTruthy();
  });

  it('shows an initial loading state', () => {
    const api = fakeProjectsApi();
    api.getProject.mockReturnValue(new Promise<ProjectDetails>(() => undefined));
    renderProjects('/projects/delivery', api);

    expect(screen.getByRole('heading', { name: 'Project' })).toBeTruthy();
    expect(
      within(screen.getByRole('main')).getByText('Loading Projects…').getAttribute('role'),
    ).toBe('status');
  });

  it('distinguishes an empty root collection from a request failure', async () => {
    const api = fakeProjectsApi();
    api.listProjects.mockResolvedValue(page([], null));
    renderProjects('/projects', api);

    expect(await screen.findByText('No root Projects are available.')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByText('Select a Project from the hierarchy on the left.')).toBeTruthy();
  });

  it('renders a not-found deep link without converting it to an empty collection', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockRejectedValue(managementError('not_found', 'request-not-found'));
    renderProjects('/projects/missing', api);

    expect(await screen.findByRole('heading', { name: 'Project not found' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Return to Projects' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenCalledWith(null, null, expect.any(AbortSignal));
  });

  it('retains loaded Projects and marks them stale after refresh fails', async () => {
    const api = fakeProjectsApi();
    api.listProjects
      .mockResolvedValueOnce(page([project('root-a', 'Alpha')], null))
      .mockRejectedValueOnce(managementError('unavailable', 'request-refresh'));
    renderProjects('/projects', api);

    expect(await screen.findByRole('link', { name: 'Alpha' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Refresh Project hierarchy' }));

    expect(
      (
        await screen.findByText('Refresh failed. Showing the last loaded Project hierarchy data.')
      ).getAttribute('role'),
    ).toBe('status');
    expect(screen.getByRole('link', { name: 'Alpha' })).toBeTruthy();
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
