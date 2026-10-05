// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createConsoleQueryClient } from '../../app/query';
import type { AuditApi, AuditFactPage, AuditFactResource } from './api';
import { AuditFilterPanel, AuditView } from './AuditView';

afterEach(cleanup);

describe('Audit view', () => {
  it('opens request-correlated evidence from an action link without deriving authority', async () => {
    const api = fakeAuditApi();
    api.listAuditFacts.mockResolvedValue(page([fact('fact-1')], null));
    renderAudit('/audit?request_identity=request-agent-command', api);

    const table = await screen.findByRole('table', { name: 'Immutable audit facts' });
    expect(api.listAuditFacts).toHaveBeenCalledWith(
      {
        actorIdentity: null,
        actorKind: null,
        occurredFromUnixMs: null,
        occurredThroughUnixMs: null,
        operation: null,
        requestIdentity: 'request-agent-command',
        targetIdentity: null,
        targetKind: null,
      },
      null,
      expect.any(AbortSignal),
    );
    expect(within(table).getByText('request-agent-command')).toBeTruthy();
    expect(screen.getByText(/Audit facts are evidence of committed operations/)).toBeTruthy();
    expect(screen.getByText(/do not grant or describe current authority/)).toBeTruthy();
    expect(screen.queryByText(/currently authorized/i)).toBeNull();
  });

  it('round-trips bounded actor, operation, target, request, and time filters in the URL', async () => {
    const api = fakeAuditApi();
    api.listAuditFacts.mockResolvedValue(page([], null));
    const { router } = renderAudit('/audit', api);
    await screen.findByText('No audit facts match the selected filters.');

    await userEvent.click(screen.getByRole('combobox', { name: 'Actor kind' }));
    await userEvent.click(screen.getByRole('option', { name: 'Agent' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Actor identity' }), 'agent-7');
    await userEvent.type(screen.getByRole('textbox', { name: 'Operation' }), 'drain_agent');
    await userEvent.type(screen.getByRole('textbox', { name: 'Target kind' }), 'agent');
    await userEvent.type(screen.getByRole('textbox', { name: 'Target identity' }), 'agent-7');
    await userEvent.type(screen.getByRole('textbox', { name: 'Request identity' }), 'request-7');
    const from = screen.getByLabelText('Occurred from');
    const through = screen.getByLabelText('Occurred through');
    fireEvent.change(from, { target: { value: '2026-10-05T09:30' } });
    fireEvent.change(through, { target: { value: '2026-10-05T10:45' } });
    await userEvent.click(screen.getByRole('button', { name: 'Apply audit filters' }));

    const fromMilliseconds = Date.parse('2026-10-05T09:30');
    const throughMilliseconds = Date.parse('2026-10-05T10:45');
    await waitFor(() =>
      expect(router.state.location.search).toBe(
        `?actor_kind=agent&actor_identity=agent-7&operation=drain_agent&target_kind=agent&target_identity=agent-7&request_identity=request-7&occurred_from_unix_ms=${fromMilliseconds}&occurred_through_unix_ms=${throughMilliseconds}`,
      ),
    );
    await waitFor(() =>
      expect(api.listAuditFacts).toHaveBeenLastCalledWith(
        {
          actorIdentity: 'agent-7',
          actorKind: 'agent',
          occurredFromUnixMs: fromMilliseconds,
          occurredThroughUnixMs: throughMilliseconds,
          operation: 'drain_agent',
          requestIdentity: 'request-7',
          targetIdentity: 'agent-7',
          targetKind: 'agent',
        },
        null,
        expect.any(AbortSignal),
      ),
    );
    expect(screen.getByText('8 active filters')).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Clear audit filters' }));
    await waitFor(() => expect(router.state.location.search).toBe(''));
  });

  it('rejects an oversized copied filter without issuing an unbounded request', async () => {
    const api = fakeAuditApi();
    renderAudit(`/audit?operation=${'x'.repeat(129)}`, api);

    expect((await screen.findByRole('alert')).textContent).toBe(
      'Operation must contain 1–128 UTF-8 bytes without surrounding whitespace or control characters.',
    );
    expect(api.listAuditFacts).not.toHaveBeenCalled();
  });

  it('keeps an invalid multibyte form value out of the URL and reports its byte bound', async () => {
    const api = fakeAuditApi();
    api.listAuditFacts.mockResolvedValue(page([], null));
    const { router } = renderAudit('/audit', api);
    await screen.findByText('No audit facts match the selected filters.');

    await userEvent.type(screen.getByRole('textbox', { name: 'Operation' }), '🔥'.repeat(33));
    await userEvent.click(screen.getByRole('button', { name: 'Apply audit filters' }));

    expect((await screen.findByRole('alert')).textContent).toBe(
      'Operation must contain 1–128 UTF-8 bytes without surrounding whitespace or control characters.',
    );
    expect(router.state.location.search).toBe('');
    expect(api.listAuditFacts).toHaveBeenCalledTimes(1);
  });

  it('keeps server order across cursor pages and distinguishes an empty page', async () => {
    const api = fakeAuditApi();
    api.listAuditFacts
      .mockResolvedValueOnce(page([fact('fact-newer'), fact('fact-same-time')], 'page-2'))
      .mockResolvedValueOnce(page([fact('fact-older')], null));
    renderAudit('/audit?target_kind=build', api);

    const facts = await screen.findByRole('table', { name: 'Immutable audit facts' });
    expect(factIds(facts)).toEqual(['fact-newer', 'fact-same-time']);
    await userEvent.click(screen.getByRole('button', { name: 'Load more audit facts' }));
    await screen.findByText('fact-older');
    expect(factIds(facts)).toEqual(['fact-newer', 'fact-same-time', 'fact-older']);
    expect(api.listAuditFacts).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ targetKind: 'build' }),
      'page-2',
      expect.any(AbortSignal),
    );

    cleanup();
    const emptyApi = fakeAuditApi();
    emptyApi.listAuditFacts.mockResolvedValue(page([], null));
    renderAudit('/audit?actor_kind=worker', emptyApi);
    expect(await screen.findByText('No audit facts match the selected filters.')).toBeTruthy();
  });
});

function renderAudit(path: string, api: AuditApi) {
  const router = createMemoryRouter(
    [
      {
        path: '/audit',
        element: (
          <>
            <AuditFilterPanel />
            <AuditView api={api} />
          </>
        ),
      },
    ],
    { initialEntries: [path] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router };
}

function fakeAuditApi() {
  return { listAuditFacts: vi.fn<AuditApi['listAuditFacts']>() };
}

function factIds(table: HTMLElement): Array<string | null> {
  return within(table)
    .getAllByRole('row')
    .slice(1)
    .map((row) => within(row).getByText(/^fact-/u).textContent);
}

function page(items: AuditFactResource[], nextCursor: string | null): AuditFactPage {
  return { items, next_cursor: nextCursor };
}

function fact(id: string): AuditFactResource {
  return {
    actor: { identity: 'agent-7', kind: 'agent' },
    id,
    idempotency_key: null,
    metadata: {},
    occurred_at_unix_ms: 1_700_000_000_000,
    operation: 'drain_agent',
    outcome: 'accepted',
    request_identity: 'request-agent-command',
    target_identity: 'agent-7',
    target_kind: 'agent',
  };
}
