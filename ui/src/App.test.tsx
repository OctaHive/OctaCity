// @vitest-environment jsdom

import { QueryClient } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { App } from './App';
import type { ReadinessProbe } from './api/readiness';
import type { OperatorAttentionApi } from './api/operatorAttention';
import { createConsoleQueryClient } from './app/query';
import { createConsoleRoutes } from './app/router';
import { CONSOLE_PATHS } from './app/routes';
import { NARROW_WORKBENCH_MEDIA_QUERY } from './app/shell/layout';
import { RouteErrorBoundary } from './app/shell/RouteErrorBoundary';

const queryClients: QueryClient[] = [];
const emptyAttentionApi: OperatorAttentionApi = {
  async listOperatorAttention() {
    return { items: [], next_cursor: null };
  },
};

afterEach(() => {
  cleanup();
  for (const client of queryClients) {
    client.clear();
  }
  queryClients.length = 0;
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.resolvedTheme;
  vi.unstubAllGlobals();
});

function renderConsole(path: string, readinessProbe: ReadinessProbe = async () => 'ready') {
  const queryClient = createConsoleQueryClient();
  queryClients.push(queryClient);
  const router = createMemoryRouter(
    createConsoleRoutes({ attentionApi: emptyAttentionApi, readinessProbe }),
    {
      initialEntries: [path],
    },
  );
  const result = render(<App queryClient={queryClient} router={router} />);
  return { ...result, router };
}

describe('App', () => {
  it('redirects the root into the Project workbench', async () => {
    const { router } = renderConsole('/');

    await screen.findByText('Server ready');
    expect(router.state.location.pathname).toBe(CONSOLE_PATHS.projects);
    expect(await screen.findByRole('heading', { level: 1, name: 'Projects' })).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Projects explorer' })).toBeTruthy();
    expect(screen.getByRole('link', { current: 'page', name: 'Projects' })).toBeTruthy();
    expect(screen.getByLabelText('Security notice')).toBeTruthy();
  });

  it('keeps Favorites before the contextual browse content', () => {
    renderConsole(CONSOLE_PATHS.projects);
    const favorites = screen.getByRole('heading', { level: 3, name: 'Favorites' });
    const browse = screen.getByRole('heading', { level: 3, name: 'Browse' });

    expect(favorites.compareDocumentPosition(browse) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
  });

  it('renders identity-only favorites in the matching contextual explorer', () => {
    const projectId = '11111111-1111-4111-8111-111111111111';
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        expanded: [],
        explorerOpen: true,
        explorerWidth: 288,
        favorites: [{ id: projectId, kind: 'project' }],
        language: 'en',
        notificationLastOpenedAt: null,
        recents: [],
        theme: 'system',
        version: 2,
      }),
    );
    renderConsole(CONSOLE_PATHS.projects);

    const favorites = screen.getByRole('region', { name: 'Favorites' });
    const link = within(favorites).getByRole('link', { name: `Open Projects ${projectId}` });
    expect(link.getAttribute('href')).toBe(`/projects/${projectId}`);
    expect(localStorage.getItem('octacity.console.preferences')).not.toContain('label');
  });

  it('uses a tool-focused explorer for Audit without generic navigation groups', async () => {
    renderConsole(CONSOLE_PATHS.audit);

    expect(await screen.findByRole('heading', { level: 1, name: /^Audit$/ })).toBeTruthy();
    const explorer = screen.getByRole('complementary', { name: 'Audit explorer' });
    expect(within(explorer).queryByRole('heading', { level: 3, name: 'Favorites' })).toBeNull();
    expect(within(explorer).queryByRole('heading', { level: 3, name: 'Browse' })).toBeNull();
    expect(await within(explorer).findByRole('form', { name: 'Audit filters' })).toBeTruthy();
    expect(
      within(screen.getByRole('main')).queryByRole('form', { name: 'Audit filters' }),
    ).toBeNull();
    expect(screen.getByRole('separator', { name: 'Resize Audit explorer' })).toBeTruthy();
  });

  it('collapses and restores the explorer with deterministic focus', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);
    const collapse = screen.getByRole('button', { name: 'Collapse Projects explorer' });

    collapse.focus();
    await user.keyboard('{Enter}');

    const open = screen.getByRole('button', { name: 'Open Projects explorer' });
    await waitFor(() => expect(document.activeElement).toBe(open));
    expect(screen.queryByRole('complementary', { name: 'Projects explorer' })).toBeNull();

    await user.keyboard('{Enter}');
    const heading = screen.getByRole('heading', { level: 2, name: 'Projects' });
    await waitFor(() => expect(document.activeElement).toBe(heading));
  });

  it('resizes the explorer by keyboard within persisted bounds', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);
    const separator = screen.getByRole('separator', { name: 'Resize Projects explorer' });

    expect(separator.getAttribute('aria-valuenow')).toBe('288');
    separator.focus();
    await user.keyboard('{End}{ArrowRight}');
    expect(separator.getAttribute('aria-valuenow')).toBe('480');
    expect(storedPreferences()).toMatchObject({ explorerWidth: 480, version: 2 });

    await user.keyboard('{Home}{ArrowLeft}');
    expect(separator.getAttribute('aria-valuenow')).toBe('240');
    expect(storedPreferences()).toMatchObject({ explorerWidth: 240, version: 2 });
  });

  it('persists the last pointer width even when pointer move and release are consecutive', () => {
    renderConsole(CONSOLE_PATHS.projects);
    const separator = screen.getByRole('separator', { name: 'Resize Projects explorer' });

    fireEvent.pointerDown(separator, { pointerId: 1 });
    fireEvent.pointerMove(separator, { clientX: 420, pointerId: 1 });
    fireEvent.pointerUp(separator, { clientX: 420, pointerId: 1 });

    expect(separator.getAttribute('aria-valuenow')).toBe('420');
    expect(storedPreferences()).toMatchObject({ explorerWidth: 420, version: 2 });
  });

  it('clamps a persisted explorer width before rendering', () => {
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({ explorerWidth: 900, version: 1 }),
    );
    renderConsole(CONSOLE_PATHS.projects);

    expect(
      screen
        .getByRole('separator', { name: 'Resize Projects explorer' })
        .getAttribute('aria-valuenow'),
    ).toBe('480');
  });

  it('navigates between large primary section controls without a pointing device', async () => {
    const user = userEvent.setup();
    const { router } = renderConsole(CONSOLE_PATHS.projects);
    const builds = screen.getByRole('link', { name: 'Builds' });

    builds.focus();
    await user.keyboard('{Enter}');

    await waitFor(() => expect(router.state.location.pathname).toBe(CONSOLE_PATHS.builds));
    expect(await screen.findByRole('heading', { level: 1, name: 'Builds' })).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Builds explorer' })).toBeTruthy();
    expect(screen.getByRole('link', { current: 'page', name: 'Builds' })).toBeTruthy();
  });

  it('opens global and section-scoped search through the same command-center frame', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);
    const globalSearch = screen.getByRole('button', { name: 'Search resources' });

    await user.click(globalSearch);
    expect(await screen.findByRole('dialog', { name: 'Search resources' })).toBeTruthy();
    expect(screen.getByText('Scope: All resources')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('searchbox', { name: 'Search query' }));
    await user.keyboard('{Escape}');
    await waitFor(() => expect(document.activeElement).toBe(globalSearch));

    await user.click(screen.getByRole('button', { name: 'Search Projects' }));
    expect(screen.getByText('Scope: Projects')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Search all resources' }));
    expect(screen.getByText('Scope: All resources')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Search all resources' })).toBeNull();
    await user.keyboard('{Escape}');

    await user.click(screen.getByRole('link', { name: 'Builds' }));
    await user.click(screen.getByRole('button', { name: 'Search Builds' }));
    expect(screen.getByText('Scope: Builds')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Search all resources' }));
    expect(screen.getByText('Scope: All resources')).toBeTruthy();

    await user.keyboard('{Escape}');
    await user.click(screen.getByRole('link', { name: 'Agents' }));
    await user.click(await screen.findByRole('button', { name: 'Search Agents' }));
    expect(screen.getByText('Scope: Agents and Agent Pools')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Search all resources' }));
    expect(screen.getByText('Scope: All resources')).toBeTruthy();
  });

  it('opens the command center with Command/Ctrl+K and contains focus until dismissal', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);
    const builds = screen.getByRole('link', { name: 'Builds' });
    builds.focus();

    await user.keyboard('{Control>}k{/Control}');

    const search = await screen.findByRole('searchbox', { name: 'Search query' });
    const close = screen.getByRole('button', { name: 'Close command center' });
    expect(document.activeElement).toBe(search);
    await user.tab();
    expect(document.activeElement).toBe(close);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(search);

    await user.keyboard('{Escape}');
    expect(document.activeElement).toBe(builds);
  });

  it('keeps narrow deep links usable until the focus-managed explorer is requested', async () => {
    const user = userEvent.setup();
    stubMatchMedia(true);
    renderConsole(CONSOLE_PATHS.builds);

    expect(await screen.findByRole('heading', { level: 1, name: 'Builds' })).toBeTruthy();
    expect(screen.queryByRole('dialog', { name: 'Builds explorer' })).toBeNull();
    const open = screen.getByRole('button', { name: 'Open Builds explorer' });

    await user.click(open);
    const explorer = screen.getByRole('dialog', { name: 'Builds explorer' });
    expect(explorer).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('heading', { level: 2, name: 'Builds' }));
    expect(screen.queryByRole('separator', { name: 'Resize Builds explorer' })).toBeNull();

    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Search Builds' }));
    await user.tab({ shift: true });
    expect(explorer.contains(document.activeElement)).toBe(true);

    await user.click(screen.getByRole('button', { name: 'Search Builds' }));
    expect(screen.getByRole('dialog', { name: 'Search resources' })).toBeTruthy();
    await user.keyboard('{Escape}');
    expect(screen.getByRole('dialog', { name: 'Builds explorer' })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Search Builds' }));

    await user.keyboard('{Escape}');
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole('button', { name: 'Open Builds explorer' }),
      ),
    );
  });

  it('exposes current-session theme and notification controls', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);

    await user.selectOptions(screen.getByRole('combobox', { name: 'Theme' }), 'dark');
    expect(document.documentElement.dataset.resolvedTheme).toBe('dark');

    const notifications = await screen.findByRole('button', { name: 'Notifications' });
    await waitFor(() => expect((notifications as HTMLButtonElement).disabled).toBe(false));
    await user.click(notifications);
    expect(screen.getByRole('dialog', { name: 'Notification center' })).toBeTruthy();
    expect(await screen.findByText('No relevant notifications.')).toBeTruthy();
    expect(screen.getByText('No commands were completed in this browser session.')).toBeTruthy();
    expect(storedPreferences()).toMatchObject({ notificationLastOpenedAt: expect.any(Number) });
  });

  it('persists language and explorer visibility in the bounded preference record', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);

    await user.click(screen.getByLabelText('Operator menu'));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'ru');
    expect(document.documentElement.lang).toBe('ru');
    expect(screen.getByRole('link', { current: 'page', name: 'Проекты' })).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Свернуть проводник раздела «Проекты»' }));
    expect(storedPreferences()).toMatchObject({ explorerOpen: false, language: 'ru' });
  });

  it('keeps the operator menu neutral and free of fabricated identity actions', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);

    await user.click(screen.getByLabelText('Operator menu'));
    expect((screen.getByRole('combobox', { name: 'Language' }) as HTMLSelectElement).value).toBe(
      'en',
    );
    expect(screen.getByText('Trusted network')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'API documentation' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Deployment information' }).getAttribute('href')).toBe(
      '#trusted-network-notice',
    );
    expect(screen.getByText('No browser identity or session is active.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /log out|sign out/i })).toBeNull();
  });

  it('reports unavailable readiness without removing the active view', async () => {
    renderConsole(CONSOLE_PATHS.projects, async () => 'unavailable');

    await screen.findByText('Server unavailable');
    expect(screen.getByRole('heading', { level: 1, name: 'Projects' })).toBeTruthy();
  });

  it.each([
    [CONSOLE_PATHS.projects, 'Projects'],
    [CONSOLE_PATHS.project.replace(':projectId', 'project-01'), 'Project'],
    [CONSOLE_PATHS.builds, 'Builds'],
    [CONSOLE_PATHS.build.replace(':buildId', 'build-01'), 'Build'],
    [CONSOLE_PATHS.agents, 'Agents'],
    [CONSOLE_PATHS.agent.replace(':agentId', 'agent-01'), 'Agent'],
    [CONSOLE_PATHS.agentPools, 'Agent Pools'],
    [CONSOLE_PATHS.agentPool.replace(':poolId', 'pool-01'), 'Agent Pool'],
    [CONSOLE_PATHS.audit, 'Audit'],
  ])('renders the declared deep link %s', async (path, heading) => {
    renderConsole(path);

    expect(await screen.findByRole('heading', { level: 1, name: heading })).toBeTruthy();
  });

  it('has no login route or personal session actions', async () => {
    const { router } = renderConsole('/login');
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Unknown console route' }),
    ).toBeTruthy();
    expect(router.state.location.pathname).toBe('/login');
    expect(screen.getByRole('combobox', { name: 'Theme' })).toBeTruthy();
    expect(screen.queryByRole('link', { name: /log in|sign in/i })).toBeNull();
  });

  it('renders an explicit not-found view inside the shell', async () => {
    renderConsole('/not-a-console-route');

    expect(
      await screen.findByRole('heading', { level: 1, name: 'Unknown console route' }),
    ).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Return to Projects' })).toBeTruthy();
  });

  it('contains unexpected render failures at the route boundary', async () => {
    function BrokenView(): never {
      throw new Error('private render detail');
    }

    const queryClient = createConsoleQueryClient();
    queryClients.push(queryClient);
    const router = createMemoryRouter(
      [{ path: '/', element: <BrokenView />, errorElement: <RouteErrorBoundary /> }],
      { initialEntries: ['/'] },
    );
    render(<App queryClient={queryClient} router={router} />);

    expect(
      await screen.findByRole('heading', { name: 'This view could not be displayed' }),
    ).toBeTruthy();
    expect(document.body.textContent).not.toContain('private render detail');
  });
});

function stubMatchMedia(narrow: boolean): void {
  vi.stubGlobal(
    'matchMedia',
    vi.fn((query: string) => ({
      addEventListener: vi.fn(),
      addListener: vi.fn(),
      dispatchEvent: vi.fn(() => false),
      matches: narrow && query === NARROW_WORKBENCH_MEDIA_QUERY,
      media: query,
      onchange: null,
      removeEventListener: vi.fn(),
      removeListener: vi.fn(),
    })),
  );
}

function storedPreferences(): Record<string, unknown> {
  return JSON.parse(localStorage.getItem('octacity.console.preferences') ?? '{}') as Record<
    string,
    unknown
  >;
}
