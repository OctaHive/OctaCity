// @vitest-environment jsdom

import { QueryClient } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';

import { App } from './App';
import type { ReadinessProbe } from './api/readiness';
import { createConsoleQueryClient } from './app/query';
import { createConsoleRoutes } from './app/router';
import { CONSOLE_PATHS } from './app/routes';
import { RouteErrorBoundary } from './app/shell/RouteErrorBoundary';

const queryClients: QueryClient[] = [];

afterEach(() => {
  cleanup();
  for (const client of queryClients) {
    client.clear();
  }
  queryClients.length = 0;
});

function renderConsole(path: string, readinessProbe: ReadinessProbe = async () => 'ready') {
  const queryClient = createConsoleQueryClient();
  queryClients.push(queryClient);
  const router = createMemoryRouter(createConsoleRoutes(readinessProbe), {
    initialEntries: [path],
  });
  const result = render(<App queryClient={queryClient} router={router} />);
  return { ...result, router };
}

describe('App', () => {
  it('redirects the root to the Project hierarchy', async () => {
    const { router } = renderConsole('/');

    await screen.findByText('Server ready');
    expect(router.state.location.pathname).toBe(CONSOLE_PATHS.projects);
    expect(screen.getByRole('heading', { name: 'Projects' })).toBeTruthy();
    expect(screen.getByLabelText('Security notice')).toBeTruthy();
  });

  it('collapses and expands navigation with the keyboard', async () => {
    const user = userEvent.setup();
    renderConsole(CONSOLE_PATHS.projects);
    const collapse = screen.getByRole('button', { name: 'Collapse navigation' });

    collapse.focus();
    await user.keyboard('{Enter}');

    const expand = screen.getByRole('button', { name: 'Expand navigation' });
    expect(expand.getAttribute('aria-expanded')).toBe('false');

    await user.keyboard('{Enter}');
    expect(
      screen.getByRole('button', { name: 'Collapse navigation' }).getAttribute('aria-expanded'),
    ).toBe('true');
  });

  it('navigates between primary routes without a pointing device', async () => {
    const user = userEvent.setup();
    const { router } = renderConsole(CONSOLE_PATHS.projects);
    const agents = screen.getByRole('link', { name: 'Agents' });

    agents.focus();
    await user.keyboard('{Enter}');

    await waitFor(() => expect(router.state.location.pathname).toBe(CONSOLE_PATHS.agents));
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Agents');
  });

  it('reports unavailable readiness without removing the active view', async () => {
    renderConsole(CONSOLE_PATHS.projects, async () => 'unavailable');

    await screen.findByText('Server unavailable');
    expect(screen.getByRole('heading', { name: 'Projects' })).toBeTruthy();
  });

  it.each([
    [CONSOLE_PATHS.projects, 'Projects'],
    [CONSOLE_PATHS.project.replace(':projectId', 'project-01'), 'Project'],
    [CONSOLE_PATHS.build.replace(':buildId', 'build-01'), 'Build'],
    [CONSOLE_PATHS.agents, 'Agents'],
    [CONSOLE_PATHS.agent.replace(':agentId', 'agent-01'), 'Agent'],
    [CONSOLE_PATHS.agentPools, 'Agent Pools'],
    [CONSOLE_PATHS.agentPool.replace(':poolId', 'pool-01'), 'Agent Pool'],
    [CONSOLE_PATHS.audit, 'Audit'],
  ])('renders the declared deep link %s', async (path, heading) => {
    renderConsole(path);

    expect(await screen.findByRole('heading', { name: heading })).toBeTruthy();
  });

  it('has no login route or theme switcher', async () => {
    const { router } = renderConsole('/login');
    expect(await screen.findByRole('heading', { name: 'Unknown console route' })).toBeTruthy();
    expect(router.state.location.pathname).toBe('/login');
    expect(screen.queryByRole('button', { name: /theme/i })).toBeNull();
    expect(screen.queryByRole('link', { name: /log in|sign in/i })).toBeNull();
  });

  it('renders an explicit not-found view inside the shell', async () => {
    renderConsole('/not-a-console-route');

    expect(await screen.findByRole('heading', { name: 'Unknown console route' })).toBeTruthy();
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
