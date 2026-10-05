// @vitest-environment jsdom

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError, type ManagementApiFailureCode } from '../api/client';
import {
  QUERY_FAILURE_KINDS,
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
  QueryRefreshingNotice,
  StaleQueryNotice,
  queryFailureKind,
} from './QueryStateNotice';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('query state presentations', () => {
  it.each([
    ['rate_limited', 'rate-limited'],
    ['forbidden', 'forbidden'],
    ['not_found', 'not-found'],
    ['conflict', 'conflict'],
    ['precondition_failed', 'conflict'],
    ['capability_unavailable', 'unavailable'],
    ['unavailable', 'unavailable'],
    ['transport_failure', 'unavailable'],
    ['internal', 'internal-error'],
    ['invalid_response', 'internal-error'],
  ] satisfies ReadonlyArray<[ManagementApiFailureCode, (typeof QUERY_FAILURE_KINDS)[number]]>)(
    'classifies %s as %s',
    (code, expected) => {
      expect(queryFailureKind(managementError(code))).toBe(expected);
    },
  );

  it('distinguishes loading, incremental refresh, and authoritative empty data', () => {
    const { rerender } = render(<QueryLoadingNotice label="Builds" />);
    expect(screen.getByRole('status').textContent).toContain('Loading Builds');

    rerender(<QueryRefreshingNotice label="Builds" />);
    expect(screen.getByRole('status').textContent).toContain(
      'Previously loaded data remains visible',
    );

    rerender(<QueryEmptyNotice>No Builds match this filter.</QueryEmptyNotice>);
    expect(screen.getByRole('status').textContent).toContain('No Builds match this filter.');
  });

  it('shows stable failure correlation without exposing another error shape', () => {
    render(
      <QueryFailureNotice
        error={managementError('forbidden')}
        onRetry={vi.fn()}
        title="Build could not be loaded."
      />,
    );

    const alert = screen.getByRole('alert');
    expect(alert.getAttribute('data-state')).toBe('forbidden');
    expect(alert.textContent).toContain('Error code: forbidden');
    expect(alert.textContent).toContain('Request ID: request-forbidden');
    expect(alert.textContent).not.toMatch(/log ?in|credential/i);
  });

  it('retains stale data messaging and prevents retry before bounded Retry-After', () => {
    vi.useFakeTimers();
    const onRetry = vi.fn();
    render(
      <StaleQueryNotice
        error={managementError('rate_limited', 7_000)}
        message="Refresh failed. Showing the last loaded Build."
        onRetry={onRetry}
      />,
    );

    expect(screen.getByRole('status').textContent).toContain('Showing the last loaded Build');
    expect(screen.getByText('Retry delay: 7 seconds')).toBeTruthy();
    const retry = screen.getByRole('button', { name: 'Retry in 7s' });
    expect((retry as HTMLButtonElement).disabled).toBe(true);

    act(() => vi.advanceTimersByTime(6_999));
    expect((retry as HTMLButtonElement).disabled).toBe(true);
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy();
    expect(onRetry).not.toHaveBeenCalled();
  });

  it('keeps one background presentation beside previously loaded data', () => {
    const onRetry = vi.fn();
    const { rerender } = render(
      <QueryBackgroundNotice error={null} fetching label="Build data" onRetry={onRetry} />,
    );
    expect(screen.getByRole('status').textContent).toContain('Refreshing Build data');

    rerender(
      <QueryBackgroundNotice
        error={managementError('unavailable')}
        fetching={false}
        label="Build data"
        onRetry={onRetry}
      />,
    );
    expect(screen.getByRole('status').textContent).toContain(
      'Refresh failed. Showing the last loaded Build data.',
    );
    expect(screen.getByText('Error code: unavailable')).toBeTruthy();
  });
});

function managementError(
  code: ManagementApiFailureCode,
  retryAfterMilliseconds: number | null = null,
) {
  return new ManagementApiError({
    code,
    message: `Safe ${code} message.`,
    requestId: `request-${code}`,
    retryAfterMilliseconds,
    status: code === 'rate_limited' ? 429 : 500,
  });
}
