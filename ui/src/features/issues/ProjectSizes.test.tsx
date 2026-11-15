// @vitest-environment jsdom
import { act, cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
import { DEFAULT_PREFERENCES } from '../../app/presentation/preferences';
import { translate } from '../../app/presentation/messages';
import { ManagementApiError } from '../../api/client';

afterEach(() => {
  cleanup();
  localStorage.clear();
});

it.each(['en', 'ru'] as const)(
  'publishes ordered Project sizes through keyboard controls (%s)',
  async (language) => {
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({ ...DEFAULT_PREFERENCES, language }),
    );
    const t = (key: Parameters<typeof translate>[1]) => translate(language, key);
    const user = userEvent.setup();
    const { api, router } = setup('/projects/delivery');
    api.publishProperties.mockImplementation(async (_project, request) => {
      const resource = {
        project_id: 'delivery',
        version: 1,
        published_at_unix_ms: 1,
        definition: request.definition,
      };
      api.planningChoices.mockResolvedValue({
        definitions: resource,
        assignees: { available: false, items: [] },
      });
      return { disposition: 'applied', resource };
    });
    await user.click(await screen.findByRole('button', { name: t('issueFields.settings') }));
    const sizes = await screen.findByRole('group', {
      name: language === 'en' ? 'Sizes' : 'Размеры',
    });
    await user.type(
      within(sizes).getByRole('textbox', { name: t('issueFields.newOption') }),
      'Extra large',
    );
    await user.type(
      within(sizes).getByRole('textbox', { name: t('issueFields.newOptionId') }),
      'xl',
    );
    await user.click(within(sizes).getByRole('button', { name: t('issueFields.addOption') }));
    const move = within(sizes).getByRole('button', {
      name: language === 'en' ? 'Move Extra large up' : 'Переместить Extra large выше',
    });
    move.focus();
    await user.keyboard('{Enter}');
    expect(document.activeElement).toBe(move);
    await user.click(screen.getByRole('button', { name: t('issueFields.review') }));
    const dialog = await screen.findByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: t('issueFields.publish') }));
    await within(dialog).findByText(t('issueFields.published'));
    const definition = api.publishProperties.mock.calls[0]![1].definition;
    expect(definition.sizes?.map(({ id }: { id: string }) => id)).toEqual([
      'small',
      'medium',
      'xl',
      'large',
    ]);
    expect(definition.sizes?.find(({ id }: { id: string }) => id === 'xl')).toEqual({
      id: 'xl',
      name: 'Extra large',
      retired: false,
    });
    expect(api.editPlanning).not.toHaveBeenCalled();
    await act(async () => {
      await router.navigate(`/projects/delivery/issues/${issue.id}`);
    });
    await user.click(await screen.findByRole('button', { name: t('planning.edit') }));
    await user.click(screen.getByRole('combobox', { name: t('classification.size') }));
    await user.click(screen.getByRole('option', { name: 'Extra large' }));
    await user.click(screen.getByRole('button', { name: t('planning.review') }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: t('planning.save') }),
    );
    expect(api.editPlanning.mock.calls[0]![2]).toMatchObject({ definition_version: 1, size: 'xl' });
  },
);

it.each([false, true])(
  'retains the reviewed size order across catalog reconciliation (order conflict=%s)',
  async (concurrentOrder) => {
    const user = userEvent.setup();
    const { api } = setup('/projects/delivery');
    api.publishProperties.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'conflict',
        status: 409,
        message: 'Size catalog changed',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Issue field settings' }));
    const sizes = await screen.findByRole('group', { name: 'Sizes' });
    await user.click(within(sizes).getByRole('button', { name: 'Move Small down' }));
    await user.click(screen.getByRole('button', { name: 'Review field definitions' }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Publish field definitions',
      }),
    );
    await screen.findByText('Size catalog changed');
    const order = concurrentOrder ? ['large', 'medium', 'small'] : ['small', 'medium', 'large'];
    api.planningChoices.mockResolvedValue({
      definitions: {
        project_id: 'delivery',
        version: 1,
        published_at_unix_ms: 2,
        definition: {
          labels: [],
          properties: [],
          sizes: order.map((id) => ({ id, name: id === 'medium' ? 'Mid' : id, retired: false })),
        },
      },
      assignees: { available: false, items: [] },
    });
    await user.click(screen.getByRole('button', { name: 'Refresh current data' }));
    const reconcile = await screen.findByRole('button', { name: 'Reconcile with current catalog' });
    await user.click(reconcile);
    if (concurrentOrder) {
      expect(screen.getByRole('button', { name: 'Review field definitions' })).toHaveProperty(
        'disabled',
        true,
      );
      await user.click(
        screen.getByRole('combobox', { name: 'Resolve catalog conflict: sizes/@order' }),
      );
      await user.click(screen.getByRole('option', { name: 'Keep my change' }));
      await user.click(reconcile);
    }
    api.publishProperties.mockImplementation(async (_project, request) => ({
      disposition: 'applied',
      resource: {
        project_id: 'delivery',
        version: 2,
        published_at_unix_ms: 3,
        definition: request.definition,
      },
    }));
    await user.click(screen.getByRole('button', { name: 'Review field definitions' }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Publish field definitions',
      }),
    );
    const definition = api.publishProperties.mock.calls[1]![1].definition;
    expect(definition.sizes?.map(({ id }: { id: string }) => id)).toEqual([
      'medium',
      'small',
      'large',
    ]);
    expect(definition.sizes?.find(({ id }: { id: string }) => id === 'medium')?.name).toBe('Mid');
  },
);

it.each(['local', 'current'] as const)(
  'resolves a size named order independently from catalog order (%s name)',
  async (nameDecision) => {
    const user = userEvent.setup();
    const { api } = setup('/projects/delivery');
    const choices = (ids: string[], name: string) =>
      ids.map((id) => ({ id, name: id === 'order' ? name : id, retired: false }));
    const catalog = {
      project_id: 'delivery',
      version: 1,
      published_at_unix_ms: 1,
      definition: {
        labels: [],
        properties: [],
        sizes: choices(['small', 'order', 'large'], 'Original'),
      },
    };
    api.planningChoices.mockResolvedValue({
      definitions: catalog,
      assignees: { available: false, items: [] },
    });
    api.publishProperties.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'conflict',
        status: 409,
        message: 'Size catalog changed',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Issue field settings' }));
    const sizes = await screen.findByRole('group', { name: 'Sizes' });
    await user.click(within(sizes).getByRole('button', { name: 'Move Original up' }));
    const name = within(sizes).getByDisplayValue('Original');
    await user.clear(name);
    await user.type(name, 'Local');
    await user.click(screen.getByRole('button', { name: 'Review field definitions' }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Publish field definitions',
      }),
    );
    await screen.findByText('Size catalog changed');
    api.planningChoices.mockResolvedValue({
      definitions: {
        ...catalog,
        version: 2,
        definition: {
          ...catalog.definition,
          sizes: choices(['large', 'small', 'order'], 'Remote'),
        },
      },
      assignees: { available: false, items: [] },
    });
    await user.click(screen.getByRole('button', { name: 'Refresh current data' }));
    const reconcile = await screen.findByRole('button', { name: 'Reconcile with current catalog' });
    await user.click(reconcile);
    await user.click(
      screen.getByRole('combobox', { name: 'Resolve catalog conflict: sizes/order' }),
    );
    await user.click(
      screen.getByRole('option', {
        name: nameDecision === 'local' ? 'Keep my change' : 'Use current value',
      }),
    );
    await user.click(
      screen.getByRole('combobox', { name: 'Resolve catalog conflict: sizes/@order' }),
    );
    await user.click(
      screen.getByRole('option', {
        name: nameDecision === 'local' ? 'Use current value' : 'Keep my change',
      }),
    );
    await user.click(reconcile);
    api.publishProperties.mockImplementation(async (_project, request) => ({
      disposition: 'applied',
      resource: { ...catalog, version: 3, definition: request.definition },
    }));
    await user.click(screen.getByRole('button', { name: 'Review field definitions' }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', {
        name: 'Publish field definitions',
      }),
    );
    const definition = api.publishProperties.mock.calls[1]![1].definition;
    expect(definition.sizes?.map(({ id }: { id: string }) => id)).toEqual(
      nameDecision === 'local' ? ['large', 'small', 'order'] : ['order', 'small', 'large'],
    );
    expect(definition.sizes?.find(({ id }: { id: string }) => id === 'order')?.name).toBe(
      nameDecision === 'local' ? 'Local' : 'Remote',
    );
  },
);
