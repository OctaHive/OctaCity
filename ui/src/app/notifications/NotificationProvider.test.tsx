// @vitest-environment jsdom

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';

import { PresentationProvider } from '../presentation/PresentationProvider';
import {
  MAX_SESSION_ACTIONS,
  NotificationProvider,
  useNotifications,
} from './NotificationProvider';

afterEach(() => cleanup());

describe('NotificationProvider', () => {
  it('announces outcomes and retains only the bounded current-session history', async () => {
    const user = userEvent.setup();
    render(
      <PresentationProvider storage={memoryStorage()}>
        <NotificationProvider>
          <NotificationHarness />
        </NotificationProvider>
      </PresentationProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Report outcomes' }));

    expect(screen.getByRole('status').textContent).toContain('Command 24');
    expect(screen.getByText(`History ${MAX_SESSION_ACTIONS}`)).toBeTruthy();
    expect(screen.queryByText('Command 0')).toBeNull();
  });

  it('replaces a prior outcome for the same in-memory intent', async () => {
    const user = userEvent.setup();
    render(
      <PresentationProvider storage={memoryStorage()}>
        <NotificationProvider>
          <ReplacementHarness />
        </NotificationProvider>
      </PresentationProvider>,
    );

    await user.click(screen.getByRole('button', { name: 'Report failure' }));
    expect(screen.getByText('History 1 failure')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Report success' }));
    expect(screen.getByText('History 1 success')).toBeTruthy();
  });
});

function NotificationHarness() {
  const { actionHistory, reportCommandOutcome } = useNotifications();
  return (
    <>
      <button
        onClick={() => {
          for (let index = 0; index < MAX_SESSION_ACTIONS + 5; index += 1) {
            reportCommandOutcome(Symbol(), `Command ${index}`, 'success');
          }
        }}
        type="button"
      >
        Report outcomes
      </button>
      <span>{`History ${actionHistory.length}`}</span>
      {actionHistory.map((item) => (
        <span key={item.id}>{item.title}</span>
      ))}
    </>
  );
}

const replacementIntent = Symbol('replacement');

function ReplacementHarness() {
  const { actionHistory, reportCommandOutcome } = useNotifications();
  return (
    <>
      <button
        onClick={() => reportCommandOutcome(replacementIntent, 'Retryable command', 'failure')}
        type="button"
      >
        Report failure
      </button>
      <button
        onClick={() => reportCommandOutcome(replacementIntent, 'Retryable command', 'success')}
        type="button"
      >
        Report success
      </button>
      <span>{`History ${actionHistory.length} ${actionHistory[0]?.kind ?? 'empty'}`}</span>
    </>
  );
}

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    get length() {
      return values.size;
    },
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}
