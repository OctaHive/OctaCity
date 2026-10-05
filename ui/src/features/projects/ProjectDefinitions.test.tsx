// @vitest-environment jsdom

import { cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { queryKeys } from '../../app/query';
import {
  configuration,
  configurationDetails,
  definition,
  definitionPage,
  fakeProjectsApi,
  managementError,
  pipelineDetails,
  project,
  renderProjects,
  repositoryDetails,
  trigger,
} from './ProjectTestSupport';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

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

  it('validates and confirms one manual Build from the selected configuration', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '44444444-4444-4444-8444-444444444444'),
    });
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuildConfigurations.mockResolvedValue(
      definitionPage([configuration('configuration-a', 'Production')], null),
    );
    api.listTriggers.mockResolvedValue(definitionPage([trigger('manual-trigger', 'manual')], null));
    api.getBuildConfiguration.mockResolvedValue(
      configurationDetails('configuration-a', 'Production', {
        deny_unknown: true,
        parameters: {
          environment: { default: null, required: true, value_type: 'string' },
          publish: { default: true, required: false, value_type: 'boolean' },
          retries: { default: 2, required: false, value_type: 'integer' },
        },
      }),
    );
    api.triggerBuild.mockResolvedValue({
      attempt_id: 'attempt-new',
      build_id: 'build-new',
      disposition: 'applied',
      outcome: 'accepted',
      ready_job_ids: ['job-new'],
      trigger_occurrence_id: 'occurrence-new',
    });
    const { queryClient } = renderProjects('/projects/delivery', api);
    queryClient.setQueryData(queryKeys.audit, []);

    await userEvent.click(await screen.findByRole('button', { name: 'View Production details' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Review Build' }));
    expect(await screen.findByText('Environment is required.')).toBeTruthy();

    await userEvent.type(screen.getByRole('textbox', { name: 'Environment' }), 'staging');
    await userEvent.click(screen.getByRole('button', { name: 'Review Build' }));

    const dialog = await screen.findByRole('dialog', { name: 'Start Build from Production?' });
    expect(within(dialog).getByText(/configuration Production version 3/)).toBeTruthy();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Start Build' }));

    expect(api.triggerBuild).toHaveBeenCalledWith(
      {
        configuration_id: 'configuration-a',
        configuration_version: 3,
        parameters: { environment: 'staging', publish: true, retries: 2 },
        priority: 0,
        source: { kind: 'default_reference' },
        trigger_id: 'manual-trigger',
        trigger_version: 3,
      },
      { 'Idempotency-Key': '44444444-4444-4444-8444-444444444444' },
    );
    expect((await within(dialog).findByRole('status')).textContent).toContain(
      'Build build-new accepted (Applied).',
    );
    expect(screen.getByRole('link', { name: 'Open Build build-new' }).getAttribute('href')).toBe(
      '/builds/build-new',
    );
    expect(queryClient.getQueryState(queryKeys.audit)?.isInvalidated).toBe(true);
    await waitFor(() => expect(api.listBuilds).toHaveBeenCalledTimes(2));
  });

  it('dismisses a manual Build confirmation without submitting its parent form', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '44444444-4444-4444-8444-444444444444'),
    });
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuildConfigurations.mockResolvedValue(
      definitionPage([configuration('configuration-a', 'Production')], null),
    );
    api.listTriggers.mockResolvedValue(definitionPage([trigger('manual-trigger', 'manual')], null));
    api.getBuildConfiguration.mockResolvedValue(
      configurationDetails('configuration-a', 'Production', {
        deny_unknown: true,
        parameters: {},
      }),
    );
    renderProjects('/projects/delivery', api);

    await userEvent.click(await screen.findByRole('button', { name: 'View Production details' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Review Build' }));
    expect(screen.getByRole('dialog', { name: 'Start Build from Production?' })).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Close confirmation' }));

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(api.triggerBuild).not.toHaveBeenCalled();
  });

  it('offers only source selections allowed by the published Repository', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '44444444-4444-4444-8444-444444444444'),
    });
    const api = fakeProjectsApi();
    api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
    api.listBuildConfigurations.mockResolvedValue(
      definitionPage([configuration('configuration-a', 'Production')], null),
    );
    api.listTriggers.mockResolvedValue(definitionPage([trigger('manual-trigger', 'manual')], null));
    api.getBuildConfiguration.mockResolvedValue(
      configurationDetails('configuration-a', 'Production'),
    );
    api.getRepository.mockResolvedValue(
      repositoryDetails('repository-a', 'Application Source', {
        allow_exact_revision: true,
        allowed_references: [],
        default_reference: null,
      }),
    );
    api.triggerBuild.mockResolvedValue({
      attempt_id: 'attempt-new',
      build_id: 'build-new',
      disposition: 'applied',
      outcome: 'accepted',
      ready_job_ids: ['job-new'],
      trigger_occurrence_id: 'occurrence-new',
    });
    renderProjects('/projects/delivery', api);

    await userEvent.click(await screen.findByRole('button', { name: 'View Production details' }));

    expect((await screen.findByRole('combobox', { name: 'Source' })).textContent).toContain(
      'Exact revision',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Review Build' }));
    expect(await screen.findByText('Exact revision is required.')).toBeTruthy();

    await userEvent.type(
      screen.getByRole('textbox', { name: 'Exact revision' }),
      '0123456789abcdef',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Review Build' }));
    const dialog = await screen.findByRole('dialog', { name: 'Start Build from Production?' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Start Build' }));

    expect(api.triggerBuild).toHaveBeenCalledWith(
      expect.objectContaining({
        source: { kind: 'exact_revision', value: '0123456789abcdef' },
      }),
      { 'Idempotency-Key': '44444444-4444-4444-8444-444444444444' },
    );
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
