// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryApi, type FactoryApi } from './api';
import { TriageReadiness } from './TriageReadiness';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
it('expires an observed readiness report without promoting native writes or probing again', async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000);
  const config = configurationFixture().configuration;
  const repository = {
    id: 'code',
    version: 1,
    project_id: config.reference.project_id,
    definition_digest: 'e'.repeat(64),
  };
  const readiness = vi.fn<FactoryApi['readiness']>(async () => ({
    configuration: config.reference,
    flow: config.flow.closure.root,
    repository,
    execution: 'ready',
    native_write: 'unknown',
    observed_at: 1000,
    expires_at: 2000,
    checks: [],
  }));
  render(
    <MemoryRouter>
      <TriageReadiness
        api={{ ...factoryApi, readiness }}
        configuration={config.reference}
        repository={repository}
      />
    </MemoryRouter>,
  );
  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: 'Check execution readiness' }));
  });
  expect(
    screen.getByText('Execution prerequisites are observed for this exact selection.'),
  ).toBeTruthy();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1001);
  });
  expect(
    screen.getByText('This readiness observation expired. Check the exact selection again.'),
  ).toBeTruthy();
  expect(
    screen.queryByText('Execution prerequisites are observed for this exact selection.'),
  ).toBeNull();
  expect(screen.getByText('Native writes remain unverified.')).toBeTruthy();
  expect(readiness).toHaveBeenCalledOnce();
});

it.each([false, true])(
  'binds readiness to the exact current subject, including a late response: %s',
  async (late) => {
    const config = configurationFixture().configuration;
    const repository = {
      id: 'code',
      version: 1,
      project_id: config.reference.project_id,
      definition_digest: 'e'.repeat(64),
    };
    let finish!: (value: Awaited<ReturnType<FactoryApi['readiness']>>) => void;
    const readiness = vi.fn<FactoryApi['readiness']>(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const api = { ...factoryApi, readiness };
    const view = render(
      <MemoryRouter>
        <TriageReadiness api={api} configuration={config.reference} repository={repository} />
      </MemoryRouter>,
    );
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Check execution readiness' }));
    });
    const response = {
      configuration: config.reference,
      flow: config.flow.closure.root,
      repository,
      execution: 'ready' as const,
      native_write: 'unknown' as const,
      observed_at: Date.now(),
      expires_at: Date.now() + 60_000,
      checks: [],
    };
    if (!late)
      await act(async () => {
        finish(response);
      });
    view.rerender(
      <MemoryRouter>
        <TriageReadiness
          api={api}
          configuration={config.reference}
          repository={{ ...repository, version: 2, definition_digest: 'f'.repeat(64) }}
        />
      </MemoryRouter>,
    );
    if (late)
      await act(async () => {
        finish(response);
      });
    expect(
      screen.queryByText('Execution prerequisites are observed for this exact selection.'),
    ).toBeNull();
    expect(readiness).toHaveBeenCalledOnce();
  },
);
