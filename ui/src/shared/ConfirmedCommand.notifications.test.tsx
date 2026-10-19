// @vitest-environment jsdom

import { QueryClient } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError, type ManagementApiFailureCode } from '../api/client';
import { NotificationProvider, useNotifications } from '../app/notifications/NotificationProvider';
import { PresentationProvider } from '../app/presentation/PresentationProvider';
import { ConfirmedCommand } from './ConfirmedCommand';

afterEach(() => cleanup());

describe('ConfirmedCommand notifications', () => {
  it.each([
    ['invalid_request', 400, 'rejected'],
    ['rate_limited', 429, 'rejected'],
    ['precondition_failed', 412, 'conflict'],
    ['internal', 500, 'failure'],
  ] as const)('classifies %s as a definitive %s outcome', async (code, status, expected) => {
    const user = userEvent.setup();
    renderCommand(async () => {
      throw apiError(code, status);
    });

    await user.click(screen.getByRole('button', { name: 'Confirm command' }));

    expect(await screen.findByText(`History 1 ${expected}`)).toBeTruthy();
    expect(localStorage.getItem('octacity.console.preferences') ?? '').not.toContain(
      'request-secret',
    );
  });

  it('announces an indeterminate transport result without inventing history, then records replay success', async () => {
    const user = userEvent.setup();
    const execute = vi
      .fn<() => Promise<{ ok: true }>>()
      .mockRejectedValueOnce(apiError('transport_failure', null))
      .mockResolvedValueOnce({ ok: true });
    renderCommand(execute);

    await user.click(screen.getByRole('button', { name: 'Confirm command' }));
    expect(await screen.findByText('History 0 empty')).toBeTruthy();
    expect(
      screen.getByText('The command outcome is unknown. Retry the same intent safely.'),
    ).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Retry same command' }));

    expect(await screen.findByText('History 1 success')).toBeTruthy();
    expect(screen.getByText('The command completed successfully.')).toBeTruthy();
  });
});

it('keeps an accepted preparation resumable without claiming completion or trapping editing', async () => {
  const execute = vi.fn().mockResolvedValueOnce({ ok: false }).mockResolvedValueOnce({ ok: true });
  const user = userEvent.setup();
  renderCommand(execute, true);
  await user.click(screen.getByRole('button', { name: 'Confirm command' }));
  expect(await screen.findByText('Preparation retained.')).toBeTruthy();
  expect(screen.getByText('History 0 empty')).toBeTruthy();
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Resume preparation' }));
  await user.click(screen.getByRole('button', { name: 'Resume preparation' }));
  expect(await screen.findByText('History 1 success')).toBeTruthy();
  expect(execute.mock.calls[0]?.[0]).toBe(execute.mock.calls[1]?.[0]);
});

function renderCommand(execute: () => Promise<{ ok: boolean }>, resumable = false) {
  const returnFocus = document.createElement('button');
  document.body.append(returnFocus);
  return render(
    <PresentationProvider>
      <NotificationProvider>
        <MemoryRouter>
          <ConfirmedCommand
            {...(resumable
              ? {
                  continuation: {
                    isComplete: (result: { ok: boolean }) => result.ok,
                    renderPending: () => 'Preparation retained.',
                    resumeLabel: 'Resume preparation',
                  },
                }
              : {})}
            confirmLabel="Confirm command"
            consequence="This executes one test command."
            execute={execute}
            invalidations={[]}
            onClose={() => undefined}
            queryClient={new QueryClient()}
            renderSuccess={() => 'Command accepted.'}
            request={{ value: 'safe' }}
            returnFocus={returnFocus}
            title="Test command"
          />
          <HistoryObserver />
        </MemoryRouter>
      </NotificationProvider>
    </PresentationProvider>,
  );
}

function HistoryObserver() {
  const { actionHistory } = useNotifications();
  return <span>{`History ${actionHistory.length} ${actionHistory[0]?.kind ?? 'empty'}`}</span>;
}

function apiError(code: ManagementApiFailureCode, status: number | null): ManagementApiError {
  return new ManagementApiError({
    code,
    message: 'Safe command failure.',
    requestId: 'request-secret',
    retryAfterMilliseconds: null,
    status,
  });
}
