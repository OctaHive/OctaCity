// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import { BuildExplorer, type BuildExplorerApi } from './BuildExplorer';

afterEach(cleanup);

describe('Build explorer', () => {
  it('expands and paginates Project, Configuration, and Build branches independently', async () => {
    const api = fakeApi();
    api.listProjects
      .mockResolvedValueOnce(
        page([project('project-a', 'Alpha'), project('project-b', 'Beta')], 'projects-next'),
      )
      .mockResolvedValueOnce(page([project('project-c', 'Gamma')], null));
    api.listBuildConfigurations.mockImplementation(async (projectId, cursor) => {
      if (projectId === 'project-a' && cursor === null) {
        return page([configuration('config-a', 'Alpha release', projectId)], 'alpha-config-next');
      }
      if (projectId === 'project-a' && cursor === 'alpha-config-next') {
        return page([configuration('config-a-2', 'Alpha nightly', projectId)], null);
      }
      return page([configuration('config-b', 'Beta release', projectId)], null);
    });
    api.listBuilds.mockImplementation(async (projectId, filters, cursor) => {
      if (filters.configurationId === 'config-a' && cursor === null) {
        return page([build('build-a', 'config-a', projectId, 'running')], 'builds-next');
      }
      if (filters.configurationId === 'config-a' && cursor === 'builds-next') {
        return page([build('build-a-2', 'config-a', projectId, 'failed')], null);
      }
      return page([], null);
    });
    renderExplorer('/builds', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Alpha' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Expand Alpha release' }));
    expect(await screen.findByRole('link', { name: 'build-a' })).toBeTruthy();
    expect(screen.getByText('Running')).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Expand Beta' }));
    expect(await screen.findByRole('button', { name: 'Expand Beta release' })).toBeTruthy();

    await userEvent.click(
      screen.getByRole('button', { name: 'Load more Build Configurations in Alpha' }),
    );
    expect(await screen.findByRole('button', { name: 'Expand Alpha nightly' })).toBeTruthy();
    expect(api.listBuildConfigurations).toHaveBeenCalledWith(
      'project-a',
      'alpha-config-next',
      expect.any(AbortSignal),
    );
    expect(api.listBuildConfigurations).not.toHaveBeenCalledWith(
      'project-b',
      'alpha-config-next',
      expect.anything(),
    );

    await userEvent.click(
      screen.getByRole('button', { name: 'Load more Builds for Alpha release' }),
    );
    expect(await screen.findByRole('link', { name: 'build-a-2' })).toBeTruthy();
    expect(screen.getByText('Failed')).toBeTruthy();
    expect(api.listBuilds).toHaveBeenCalledWith(
      'project-a',
      { configurationId: 'config-a', state: null },
      'builds-next',
      expect.any(AbortSignal),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Load more Projects' }));
    expect(await screen.findByRole('button', { name: 'Expand Gamma' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenLastCalledWith(
      null,
      'projects-next',
      expect.any(AbortSignal),
    );
  });

  it('discovers Builds owned by nested Projects without flattening the Project hierarchy', async () => {
    const api = fakeApi();
    api.listProjects.mockImplementation(async (parentId) => {
      if (parentId === null) {
        return page([project('root-project', 'Root Project', null, true)], null);
      }
      if (parentId === 'root-project') {
        return page([project('child-project', 'Child Project', 'root-project')], null);
      }
      return page([], null);
    });
    api.listBuildConfigurations.mockImplementation(async (projectId) =>
      projectId === 'child-project'
        ? page([configuration('child-config', 'Child release', projectId)], null)
        : page([], null),
    );
    api.listBuilds.mockResolvedValue(
      page([build('child-build', 'child-config', 'child-project', 'running')], null),
    );
    renderExplorer('/builds', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Root Project' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Expand Child Project' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Expand Child release' }));

    expect(await screen.findByRole('link', { name: 'child-build' })).toBeTruthy();
    expect(api.listProjects).toHaveBeenCalledWith('root-project', null, expect.any(AbortSignal));
    expect(api.listBuildConfigurations).toHaveBeenCalledWith(
      'child-project',
      null,
      expect.any(AbortSignal),
    );
  });

  it('distinguishes empty branches and retains stale branch data after pagination fails', async () => {
    const api = fakeApi();
    api.listProjects.mockResolvedValue(
      page([project('empty-project', 'Empty'), project('stale-project', 'Stale')], null),
    );
    api.listBuildConfigurations.mockImplementation(async (projectId, cursor) => {
      if (projectId === 'empty-project') return page([], null);
      if (cursor === null) {
        return page(
          [configuration('stale-config', 'Stale release', projectId)],
          'stale-config-next',
        );
      }
      throw managementError('request-config-pagination');
    });
    api.listBuilds.mockResolvedValue(page([], null));
    renderExplorer('/builds', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Empty' }));
    expect(await screen.findByText('No Build Configurations in Empty.')).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Expand Stale' }));
    const configurationToggle = await screen.findByRole('button', { name: 'Expand Stale release' });
    await userEvent.click(configurationToggle);
    expect(await screen.findByText('No Builds for Stale release.')).toBeTruthy();

    await userEvent.click(
      screen.getByRole('button', { name: 'Load more Build Configurations in Stale' }),
    );
    expect(
      await screen.findByText(
        'Refresh failed. Showing the last loaded Stale Build Configurations.',
      ),
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Collapse Stale release' })).toBeTruthy();
  });

  it('reveals a selected Build and its unloaded ancestors from a deep link', async () => {
    const api = fakeApi();
    configureSelectedBuild(api);
    api.getProject.mockResolvedValue({
      ancestors: [projectResource('root-project', 'Root Project')],
      project: projectResource('selected-project', 'Selected Project', 'root-project'),
    });
    api.listProjects.mockImplementation(async (parentId) => {
      if (parentId === null) return page([project('other-project', 'Other Project')], null);
      if (parentId === 'root-project') {
        return page([project('other-child', 'Other Child', 'root-project')], null);
      }
      return page([], null);
    });
    api.listBuildConfigurations.mockResolvedValue(
      page([configuration('other-config', 'Other Configuration', 'selected-project')], null),
    );
    api.listBuilds.mockResolvedValue(
      page([build('other-build', 'selected-config', 'selected-project', 'running')], null),
    );
    renderExplorer('/builds/selected-build', api);

    expect(await screen.findByRole('button', { name: 'Collapse Root Project' })).toBeTruthy();
    expect(await screen.findByRole('button', { name: 'Collapse Selected Project' })).toBeTruthy();
    expect(
      await screen.findByRole('button', { name: 'Collapse Selected Configuration' }),
    ).toBeTruthy();
    const selected = await screen.findByRole('link', { name: 'selected-build' });
    expect(selected.getAttribute('aria-current')).toBe('page');
    expect(selected.getAttribute('href')).toBe('/builds/selected-build');
    expect(screen.getByText('Succeeded')).toBeTruthy();
    expect(api.listBuilds).toHaveBeenCalledWith(
      'selected-project',
      { configurationId: 'selected-config', state: null },
      null,
      expect.any(AbortSignal),
    );
  });

  it('keeps loaded hierarchy nodes mounted and cached while selecting a Build', async () => {
    const api = fakeApi();
    configureSelectedBuild(api);
    api.listProjects.mockResolvedValue(
      page([project('selected-project', 'Selected Project')], null),
    );
    api.listBuildConfigurations.mockResolvedValue(
      page(
        [configuration('selected-config', 'Selected Configuration', 'selected-project', 7)],
        null,
      ),
    );
    api.listBuilds.mockResolvedValue(
      page([build('selected-build', 'selected-config', 'selected-project', 'succeeded')], null),
    );
    const { router } = renderExplorer('/builds', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Selected Project' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Expand Selected Configuration' }),
    );
    const hierarchy = await screen.findByRole('tree', { name: 'Build hierarchy' });
    expect(rootProjectReads(api)).toBe(1);
    expect(api.listBuildConfigurations).toHaveBeenCalledTimes(1);
    expect(api.listBuilds).toHaveBeenCalledTimes(1);
    await userEvent.click(await screen.findByRole('link', { name: 'selected-build' }));

    await waitFor(() => expect(router.state.location.pathname).toBe('/builds/selected-build'));
    await waitFor(() =>
      expect(
        screen.getByRole('link', { name: 'selected-build' }).getAttribute('aria-current'),
      ).toBe('page'),
    );
    expect(screen.getByRole('tree', { name: 'Build hierarchy' })).toBe(hierarchy);
    expect(rootProjectReads(api)).toBe(1);
    expect(api.listBuildConfigurations).toHaveBeenCalledTimes(1);
    expect(api.listBuilds).toHaveBeenCalledTimes(1);
  });

  it('reuses loaded branches when an operator collapses and expands them again', async () => {
    const api = fakeApi();
    api.listProjects.mockResolvedValue(
      page([project('selected-project', 'Selected Project')], null),
    );
    api.listBuildConfigurations.mockResolvedValue(
      page([configuration('selected-config', 'Selected Configuration', 'selected-project')], null),
    );
    api.listBuilds.mockResolvedValue(
      page([build('selected-build', 'selected-config', 'selected-project', 'succeeded')], null),
    );
    const { queryClient } = renderExplorer('/builds', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Selected Project' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Expand Selected Configuration' }),
    );
    expect(await screen.findByRole('link', { name: 'selected-build' })).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Collapse Selected Project' }));
    await userEvent.click(screen.getByRole('button', { name: 'Expand Selected Project' }));
    expect(
      await screen.findByRole('button', { name: 'Collapse Selected Configuration' }),
    ).toBeTruthy();
    expect(await screen.findByRole('link', { name: 'selected-build' })).toBeTruthy();

    expect(api.listBuildConfigurations).toHaveBeenCalledTimes(1);
    expect(api.listBuilds).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByRole('button', { name: 'Collapse Selected Project' }));
    await act(async () => {
      await queryClient.invalidateQueries({
        exact: true,
        queryKey: queryKeys.projectBuildConfigurations('selected-project'),
      });
    });
    await userEvent.click(screen.getByRole('button', { name: 'Expand Selected Project' }));
    await waitFor(() => expect(api.listBuildConfigurations).toHaveBeenCalledTimes(2));
    expect(api.listBuilds).toHaveBeenCalledTimes(1);
  });

  it('keeps cached selected-path refreshes from displacing the hierarchy', async () => {
    const api = fakeApi();
    configureSelectedBuild(api);
    api.getBuild.mockImplementation(pendingRead);
    api.getProject.mockImplementation(pendingRead);
    api.getBuildConfiguration.mockImplementation(pendingRead);
    api.listProjects.mockImplementation((parentId) =>
      parentId === null ? Promise.resolve(page([], null)) : pendingRead(),
    );
    renderExplorer('/builds/selected-build', api, (queryClient) => {
      queryClient.setQueryData(queryKeys.build('selected-build'), {
        configuration_id: 'selected-config',
        configuration_version: 7,
        id: 'selected-build',
        project_id: 'selected-project',
        state: 'succeeded',
      });
      queryClient.setQueryData(queryKeys.project('selected-project'), {
        ancestors: [],
        project: projectResource('selected-project', 'Selected Project'),
      });
      queryClient.setQueryData(
        queryKeys.buildConfiguration('selected-config', 7),
        configuration('selected-config', 'Selected Configuration', 'selected-project', 7),
      );
      queryClient.setQueryData(queryKeys.projectChildren('selected-project'), {
        pageParams: [null],
        pages: [page([], null)],
      });
    });

    expect(await screen.findByRole('link', { name: 'selected-build' })).toBeTruthy();
    expect(
      screen.queryByText('Refreshing selected Build path. Previously loaded data remains visible.'),
    ).toBeNull();
  });

  it('reports a selected Build lookup failure with request correlation while retaining browsing', async () => {
    const api = fakeApi();
    api.getBuild.mockRejectedValue(managementError('request-selected-build'));
    renderExplorer('/builds/selected-build', api);

    const selectedBuildFailure = await screen.findByRole('alert');
    expect(selectedBuildFailure.textContent).toContain('Selected Build could not be loaded.');
    expect(selectedBuildFailure.textContent).toContain('Error code: unavailable');
    expect(selectedBuildFailure.textContent).toContain('Request ID: request-selected-build');
    expect(await screen.findByText('No Project branches are available.')).toBeTruthy();
  });

  it('reports selected Project and Configuration lookup failures instead of hiding the path', async () => {
    const projectApi = fakeApi();
    configureSelectedBuild(projectApi);
    projectApi.getProject.mockRejectedValue(managementError('request-selected-project'));
    renderExplorer('/builds/selected-build', projectApi);
    const selectedProjectFailure = await screen.findByRole('alert');
    expect(selectedProjectFailure.textContent).toContain('Selected Project could not be loaded.');
    expect(selectedProjectFailure.textContent).toContain('Error code: unavailable');
    expect(selectedProjectFailure.textContent).toContain('Request ID: request-selected-project');
    cleanup();

    const configurationApi = fakeApi();
    configureSelectedBuild(configurationApi);
    configurationApi.getBuildConfiguration.mockRejectedValue(
      managementError('request-selected-configuration'),
    );
    renderExplorer('/builds/selected-build', configurationApi);
    const selectedConfigurationFailure = await screen.findByRole('alert');
    expect(selectedConfigurationFailure.textContent).toContain(
      'Selected Build Configuration could not be loaded.',
    );
    expect(selectedConfigurationFailure.textContent).toContain('Error code: unavailable');
    expect(selectedConfigurationFailure.textContent).toContain(
      'Request ID: request-selected-configuration',
    );
  });

  it('retains a revealed selected path when an ancestor refresh fails', async () => {
    const api = fakeApi();
    configureSelectedBuild(api);
    api.getProject
      .mockResolvedValueOnce({
        ancestors: [],
        project: projectResource('selected-project', 'Selected Project'),
      })
      .mockRejectedValueOnce(managementError('request-selected-project-refresh'));
    const { queryClient } = renderExplorer('/builds/selected-build', api);
    expect(await screen.findByRole('link', { name: 'selected-build' })).toBeTruthy();

    await act(async () => {
      await queryClient.refetchQueries({ queryKey: queryKeys.project('selected-project') });
    });

    expect(
      await screen.findByText('Refresh failed. Showing the last loaded selected Build path.'),
    ).toBeTruthy();
    expect(screen.getByRole('link', { name: 'selected-build' })).toBeTruthy();
  });

  it('states that explorer results are bounded rather than globally complete', async () => {
    const api = fakeApi();
    renderExplorer('/builds', api);

    expect(
      screen.getByText(
        'Loaded bounded pages are shown. Use search to find Builds outside expanded branches.',
      ),
    ).toBeTruthy();
    expect(await screen.findByText('No Project branches are available.')).toBeTruthy();
  });
});

function renderExplorer(
  path: string,
  api: BuildExplorerApi,
  prepare?: (queryClient: ReturnType<typeof createConsoleQueryClient>) => void,
) {
  const router = createMemoryRouter([{ path: '*', element: <BuildExplorer api={api} /> }], {
    initialEntries: [path],
  });
  const queryClient = createConsoleQueryClient();
  prepare?.(queryClient);
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { queryClient, router };
}

function pendingRead<T>(): Promise<T> {
  return new Promise(() => undefined);
}

function rootProjectReads(api: ReturnType<typeof fakeApi>): number {
  return api.listProjects.mock.calls.filter(([parentId]) => parentId === null).length;
}

function fakeApi() {
  return {
    getBuild: vi.fn<BuildExplorerApi['getBuild']>(),
    getBuildConfiguration: vi.fn<BuildExplorerApi['getBuildConfiguration']>(),
    getProject: vi.fn<BuildExplorerApi['getProject']>(),
    listBuildConfigurations: vi
      .fn<BuildExplorerApi['listBuildConfigurations']>()
      .mockResolvedValue(page([], null)),
    listBuilds: vi.fn<BuildExplorerApi['listBuilds']>().mockResolvedValue(page([], null)),
    listProjects: vi.fn<BuildExplorerApi['listProjects']>().mockResolvedValue(page([], null)),
  };
}

function page<T>(items: T[], nextCursor: string | null) {
  return { items, next_cursor: nextCursor };
}

function project(id: string, name: string, parentId: string | null = null, hasChildren = false) {
  return { has_children: hasChildren, id, name, parent_id: parentId };
}

function projectResource(id: string, name: string, parentId: string | null = null) {
  return {
    created_at_unix_ms: 1,
    id,
    name,
    parent_id: parentId,
    updated_at_unix_ms: 1,
    version: 1,
  };
}

function configuration(id: string, name: string, projectId: string, version = 1) {
  return { id, name, project_id: projectId, version };
}

function build(
  id: string,
  configurationId: string,
  projectId: string,
  state: 'failed' | 'running' | 'succeeded',
) {
  return { configuration_id: configurationId, id, project_id: projectId, state };
}

function configureSelectedBuild(api: ReturnType<typeof fakeApi>) {
  api.getBuild.mockResolvedValue({
    configuration_id: 'selected-config',
    configuration_version: 7,
    id: 'selected-build',
    project_id: 'selected-project',
    state: 'succeeded',
  });
  api.getProject.mockResolvedValue({
    ancestors: [],
    project: projectResource('selected-project', 'Selected Project'),
  });
  api.getBuildConfiguration.mockResolvedValue(
    configuration('selected-config', 'Selected Configuration', 'selected-project', 7),
  );
}

function managementError(requestId: string) {
  return new ManagementApiError({
    code: 'unavailable',
    message: 'Safe failure.',
    requestId,
    retryAfterMilliseconds: null,
    status: 503,
  });
}
