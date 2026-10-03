// @vitest-environment jsdom

import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import {
  configuration,
  definition,
  definitionPage,
  fakeProjectsApi,
  managementError,
  pipelineDetails,
  project,
  renderProjects,
  trigger,
} from './ProjectTestSupport';

afterEach(cleanup);

describe('Project definitions', () => {
  it('discovers current definitions and reads exact details without identifier entry', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listPipelines.mockResolvedValue(
      definitionPage([definition('pipeline-a', 'Delivery Pipeline')], null),
    );
    api.listRepositories.mockResolvedValue(
      definitionPage([definition('repository-a', 'Application Source')], null),
    );
    api.listBuildConfigurations.mockResolvedValue(
      definitionPage([configuration('configuration-a', 'Production')], null),
    );
    api.listTriggers.mockResolvedValue(
      definitionPage(
        [
          trigger('manual-trigger', 'manual'),
          trigger('scheduled-trigger', 'scheduled'),
          trigger('internal-trigger', 'internal'),
        ],
        null,
      ),
    );
    api.getPipeline.mockResolvedValue(pipelineDetails('pipeline-a', 'Delivery Pipeline'));
    const { container } = renderProjects('/projects/delivery', api);

    expect(await screen.findByRole('heading', { name: 'Current definitions' })).toBeTruthy();
    expect(await screen.findByText('Delivery Pipeline')).toBeTruthy();
    expect(screen.getByText('Application Source')).toBeTruthy();
    expect(screen.getByText('Production')).toBeTruthy();
    expect(screen.getByText('Manual trigger')).toBeTruthy();
    expect(screen.getByText('Scheduled trigger')).toBeTruthy();
    expect(screen.getByText('Internal trigger')).toBeTruthy();
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(container.querySelector('input')).toBeNull();

    await userEvent.click(screen.getByRole('button', { name: 'View Delivery Pipeline details' }));

    expect(api.getPipeline).toHaveBeenCalledWith('pipeline-a', 3, expect.any(AbortSignal));
    expect(await screen.findByText('0 Jobs')).toBeTruthy();
  });

  it('paginates every definition section with an independent server cursor', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listPipelines
      .mockResolvedValueOnce(
        definitionPage([definition('pipeline-a', 'Pipeline A')], 'pipelines-2'),
      )
      .mockResolvedValueOnce(definitionPage([definition('pipeline-b', 'Pipeline B')], null));
    api.listRepositories
      .mockResolvedValueOnce(
        definitionPage([definition('repository-a', 'Repository A')], 'repositories-2'),
      )
      .mockResolvedValueOnce(definitionPage([definition('repository-b', 'Repository B')], null));
    api.listBuildConfigurations
      .mockResolvedValueOnce(
        definitionPage([configuration('configuration-a', 'Configuration A')], 'configurations-2'),
      )
      .mockResolvedValueOnce(
        definitionPage([configuration('configuration-b', 'Configuration B')], null),
      );
    api.listTriggers
      .mockResolvedValueOnce(definitionPage([trigger('trigger-a', 'manual')], 'triggers-2'))
      .mockResolvedValueOnce(definitionPage([trigger('trigger-b', 'internal')], null));
    renderProjects('/projects/delivery', api);

    const pipelines = await screen.findByRole('region', { name: 'Pipelines' });
    const repositories = screen.getByRole('region', { name: 'Repositories' });
    const configurations = screen.getByRole('region', { name: 'Build Configurations' });
    const triggers = screen.getByRole('region', { name: 'Triggers' });

    await userEvent.click(
      await within(pipelines).findByRole('button', { name: 'Load more Pipelines' }),
    );
    expect(await within(pipelines).findByText('Pipeline B')).toBeTruthy();
    expect(api.listRepositories).toHaveBeenCalledTimes(1);

    await userEvent.click(
      await within(repositories).findByRole('button', { name: 'Load more Repositories' }),
    );
    expect(await within(repositories).findByText('Repository B')).toBeTruthy();
    expect(api.listBuildConfigurations).toHaveBeenCalledTimes(1);

    await userEvent.click(
      await within(configurations).findByRole('button', {
        name: 'Load more Build Configurations',
      }),
    );
    expect(await within(configurations).findByText('Configuration B')).toBeTruthy();
    expect(api.listTriggers).toHaveBeenCalledTimes(1);

    await userEvent.click(
      await within(triggers).findByRole('button', { name: 'Load more Triggers' }),
    );
    expect(await within(triggers).findByText('Internal trigger')).toBeTruthy();
  });

  it('retains definitions and exposes a retry when the next page fails', async () => {
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listPipelines
      .mockResolvedValueOnce(
        definitionPage([definition('pipeline-a', 'Pipeline A')], 'pipelines-2'),
      )
      .mockRejectedValueOnce(managementError('unavailable', 'request-pipelines'));
    renderProjects('/projects/delivery', api);

    const pipelines = await screen.findByRole('region', { name: 'Pipelines' });
    await userEvent.click(
      await within(pipelines).findByRole('button', { name: 'Load more Pipelines' }),
    );

    expect(await within(pipelines).findByText('Pipeline A')).toBeTruthy();
    expect(
      within(pipelines).getByText('Refresh failed. Showing the last loaded Pipelines data.'),
    ).toBeTruthy();
    expect(within(pipelines).getByRole('button', { name: 'Retry' })).toBeTruthy();
  });
});
