// @vitest-environment jsdom

import { cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { formatTimestamp } from '../../shared/display';
import {
  BUILD_A,
  BUILD_B,
  BUILD_C,
  CONFIGURATION_A,
  build,
  configuration,
  definitionPage,
  fakeProjectsApi,
  managementError,
  project,
  renderProjects,
} from './ProjectTestSupport';

afterEach(cleanup);

describe('Project Builds', () => {
  it('round-trips Build filters through the URL and omits cleared filters', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuildConfigurations.mockResolvedValue(
      definitionPage([configuration(CONFIGURATION_A, 'Configuration A')], null),
    );
    const { router } = renderProjects(
      `/projects/delivery?configuration_id=${CONFIGURATION_A}&state=failed`,
      api,
    );

    await screen.findByRole('heading', { name: 'Recent Builds' });
    await waitFor(() =>
      expect(api.listBuilds).toHaveBeenCalledWith(
        'delivery',
        { configurationId: CONFIGURATION_A, state: 'failed' },
        null,
        expect.any(AbortSignal),
      ),
    );

    await userEvent.click(screen.getByRole('combobox', { name: 'Build state' }));
    await userEvent.click(screen.getByRole('option', { name: 'Running' }));
    await waitFor(() =>
      expect(router.state.location.search).toBe(
        `?configuration_id=${CONFIGURATION_A}&state=running`,
      ),
    );
    await userEvent.click(screen.getByRole('combobox', { name: 'Build Configuration' }));
    await userEvent.click(screen.getByRole('option', { name: 'All configurations' }));
    await waitFor(() => expect(router.state.location.search).toBe('?state=running'));
    await userEvent.click(screen.getByRole('combobox', { name: 'Build state' }));
    await userEvent.click(screen.getByRole('option', { name: 'All states' }));
    await waitFor(() => expect(router.state.location.search).toBe(''));
  });

  it('preserves equal-time server ordering across cursor pages and opens a Build', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuilds
      .mockResolvedValueOnce(
        definitionPage(
          [
            build(BUILD_B, CONFIGURATION_A, 'failed', 1_700_000_000_123),
            build(BUILD_A, CONFIGURATION_A, 'succeeded', 1_700_000_000_123),
          ],
          'builds-page-2',
        ),
      )
      .mockResolvedValueOnce(
        definitionPage([build(BUILD_C, CONFIGURATION_A, 'running', 1_699_999_999_000)], null),
      );
    const { router } = renderProjects('/projects/delivery', api);

    const builds = await screen.findByRole('region', { name: 'Recent Builds' });
    expect((await within(builds).findAllByRole('link')).map((link) => link.textContent)).toEqual([
      BUILD_B,
      BUILD_A,
    ]);
    expect(
      within(builds).getAllByText(formatTimestamp(1_700_000_000_123, 'en-US').display),
    ).toHaveLength(2);

    await userEvent.click(await within(builds).findByRole('button', { name: 'Load more Builds' }));
    expect(await within(builds).findByRole('link', { name: BUILD_C })).toBeTruthy();
    expect(
      within(builds)
        .getAllByRole('link')
        .map((link) => link.textContent),
    ).toEqual([BUILD_B, BUILD_A, BUILD_C]);

    await userEvent.click(within(builds).getByRole('link', { name: BUILD_B }));
    await waitFor(() => expect(router.state.location.pathname).toBe(`/builds/${BUILD_B}`));
  });

  it('canonicalizes malformed copied filters before the URL can be shared', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    const { router } = renderProjects(
      '/projects/delivery?configuration_id=not-a-uuid&state=unknown&view=compact',
      api,
    );

    await waitFor(() =>
      expect(api.listBuilds).toHaveBeenCalledWith(
        'delivery',
        { configurationId: null, state: null },
        null,
        expect.any(AbortSignal),
      ),
    );
    await waitFor(() => expect(router.state.location.search).toBe('?view=compact'));
  });

  it('shows filter-option and Build pagination failures without discarding loaded data', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuildConfigurations.mockRejectedValue(
      managementError('unavailable', 'request-configurations'),
    );
    api.listBuilds
      .mockResolvedValueOnce(
        definitionPage(
          [build(BUILD_A, CONFIGURATION_A, 'running', 1_700_000_000_000)],
          'builds-page-2',
        ),
      )
      .mockRejectedValueOnce(managementError('unavailable', 'request-builds'));
    renderProjects('/projects/delivery', api);

    const builds = await screen.findByRole('region', { name: 'Recent Builds' });
    expect(
      await within(builds).findByText('Build Configuration filter options could not be loaded.'),
    ).toBeTruthy();
    await userEvent.click(await within(builds).findByRole('button', { name: 'Load more Builds' }));

    expect(within(builds).getByRole('link', { name: BUILD_A })).toBeTruthy();
    expect(
      await within(builds).findByText('Refresh failed. Showing the last loaded Build data.'),
    ).toBeTruthy();
  });
});
