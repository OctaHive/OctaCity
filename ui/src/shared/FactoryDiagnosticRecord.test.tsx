// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { DiagnosticRecordDetails } from './FactoryDiagnosticRecord';

afterEach(cleanup);
it('renders safe profile selection metadata without unknown instruction or provider content', () => {
  const reference = { project_id: 'project', id: 'profile', version: 1, digest: 'a'.repeat(64) };
  const record: components['schemas']['FactoryProfileSelectionProjection'] = {
    id: 'b'.repeat(64),
    input_digest: 'c'.repeat(64),
    flow_run_id: 'flow',
    cycle_id: 'cycle',
    node: 'custom-stage',
    generation: 0,
    predecessor: null,
    control_id: null,
    pending_control: null,
    method: 'manual',
    policy: null,
    rules: null,
    signal: null,
    role: reference,
    candidate: null,
    profile: null,
    runtime: null,
    model: null,
    thinking_level: null,
    selector_error: null,
    recovery: null,
    started_at: 1,
    deadline: 100,
    superseded: false,
    eligible_candidates: [{ candidate: 'custom-profile', profile: reference }],
    builds: [],
  };
  render(
    <DiagnosticRecordDetails
      item={{
        kind: 'profile_selection',
        record: Object.assign(record, {
          instructions: 'private-instructions',
          provider_payload: 'private-provider-content',
        }),
      }}
    />,
  );
  expect(screen.getByText('manual')).toBeTruthy();
  expect(screen.getByText('custom-stage')).toBeTruthy();
  expect(screen.getByText('custom-profile')).toBeTruthy();
  expect(screen.queryByText('private-instructions')).toBeNull();
  expect(screen.queryByText('private-provider-content')).toBeNull();
});
