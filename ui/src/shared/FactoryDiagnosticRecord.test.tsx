// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { triageCompletion } from '../../tests/fixtures/factoryTriage';
import { DiagnosticRecordDetails, SafeFactoryMetadata } from './FactoryDiagnosticRecord';

afterEach(cleanup);
it('shows unpriced monetary metadata while excluding unrelated private members', () => {
  render(
    <SafeFactoryMetadata
      value={{
        cost_micro_units: {
          status: 'unknown',
          known_micro_units: 7,
          private_payload: 'private-cost-canary',
        },
      }}
    />,
  );
  expect(screen.getByText('unknown')).toBeTruthy();
  expect(screen.getByText('known micro units')).toBeTruthy();
  expect(screen.getByText('7')).toBeTruthy();
  expect(screen.queryByText('private-cost-canary')).toBeNull();
});
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

it('renders both public triage assessments without private context or invented reproduction evidence', () => {
  render(
    <>
      <DiagnosticRecordDetails
        item={{ kind: 'node_completion', record: triageCompletion('eligibility') }}
      />
      <DiagnosticRecordDetails
        item={{ kind: 'node_completion', record: triageCompletion('classification') }}
      />
    </>,
  );
  expect(screen.getByRole('heading', { name: 'Project assessment' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Systems analyst assessment' })).toBeTruthy();
  expect(screen.getByText('Scope needs an operator decision')).toBeTruthy();
  expect(screen.getByText('80')).toBeTruthy();
  expect(screen.getByText('High')).toBeTruthy();
  expect(screen.getByText('Needs research')).toBeTruthy();
  expect(screen.getByText('No retained reproduction evidence')).toBeTruthy();
  expect(screen.getByText('scoped-project-issues')).toBeTruthy();
  expect(screen.getByText('only-this-project')).toBeTruthy();
  expect(screen.getByText('Coverage unknown')).toBeTruthy();
  for (const value of [
    'hidden-context',
    'hidden-reasoning',
    'hidden-candidate-payload',
    'hidden-token',
  ])
    expect(screen.queryByText(value)).toBeNull();
});

it.each(['pending', 'claimed', 'delivered', 'failed'] as const)(
  'keeps a %s Reporter receipt separate from the accepted decision',
  (state) => {
    render(
      <>
        <DiagnosticRecordDetails
          item={{ kind: 'node_completion', record: triageCompletion('classification') }}
        />
        <DiagnosticRecordDetails
          item={{
            kind: 'effect_delivery',
            record: {
              id: 'a'.repeat(64),
              record_id: 'b'.repeat(64),
              kind: 'work.status.projection',
              input_digest: 'c'.repeat(64),
              state,
              failure: state === 'failed' ? 'forbidden' : null,
              attempt: 2,
              available_at: 40,
              recorded_at: 42,
            },
          }}
        />
      </>,
    );
    expect(screen.getByRole('heading', { name: 'Systems analyst assessment' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Reporter delivery' })).toBeTruthy();
    expect(
      screen.getByText(
        {
          pending: 'Queued; delivery not confirmed',
          claimed: 'In progress; delivery not confirmed',
          delivered: 'Delivery confirmed',
          failed: 'Delivery failed',
        }[state],
      ),
    ).toBeTruthy();
    if (state === 'failed')
      expect(
        screen.getByText('Write authority unavailable; review the connection grant'),
      ).toBeTruthy();
    expect(screen.queryByRole('button', { name: /retry/i })).toBeNull();
  },
);

it('links consumed MCP observations to the original Build and retains exact dependency Run identities', () => {
  const eligibility = triageCompletion('eligibility');
  const classification = triageCompletion('classification');
  eligibility.public_result = {
    accepted: true,
    decision: 'eligible',
    reason: 'Reviewed project fit',
    evidence: [],
    lookup: {
      coverage: 'unknown',
      status: 'observed',
      capture_status: 'complete',
      observations: [],
      producer: { build_id: 'lookup-build', attempt_id: 'lookup-attempt', job_id: 'lookup-job' },
      trace_artifact_id: 'lookup-trace',
      provenance_artifact_id: 'lookup-provenance',
    },
  };
  classification.public_result = {
    accepted: true,
    admission_priority: 25,
    admission_risk: 'low',
    assessment: {
      priority_assessment: 80,
      work_kind: 'defect',
      reproducibility: 'reproduced',
      dependencies: [
        {
          run_id: 'dependency-run',
          work_id: 'dependency-work',
          work_digest: 'd'.repeat(64),
          terminal: 'completed',
        },
      ],
    },
    evidence: {
      observed: {
        status: 'completed',
        finding: 'matched',
        owner_input_digest: 'e'.repeat(64),
        record_digest: 'f'.repeat(64),
        fresh_until: 2000,
        policy: {
          tool: { identity: 'reproducer', version: 'v1', digest: 'a'.repeat(64) },
          environment: {
            identity: 'reviewed-test-environment',
            version: 'v1',
            digest: 'a'.repeat(64),
          },
          criterion: {
            identity: 'original-reproduction-criterion',
            version: 'v1',
            digest: 'b'.repeat(64),
          },
        },
        scope: {
          input_digest: 'b'.repeat(64),
          subject: {
            project_id: 'original-project',
            repository_id: 'original-repository',
            base_revision: 'original-revision',
          },
          command: { identity: 'reproducer', version: 'v1', digest: 'a'.repeat(64) },
          output: 'reproduction-report',
        },
        producer: {
          build_id: 'reproduction-build',
          attempt_id: 'reproduction-attempt',
          job_id: 'reproduction-job',
        },
      },
    },
  };
  render(
    <MemoryRouter>
      <DiagnosticRecordDetails item={{ kind: 'node_completion', record: eligibility }} />
      <DiagnosticRecordDetails item={{ kind: 'node_completion', record: classification }} />
    </MemoryRouter>,
  );
  expect(
    screen.getByRole('link', { name: 'Open retained lookup evidence' }).getAttribute('href'),
  ).toBe('/builds/lookup-build');
  expect(screen.getByRole('link', { name: 'dependency-run' }).getAttribute('href')).toBe(
    '/factory/dependency-run',
  );
  expect(screen.getByText('dependency-work')).toBeTruthy();
  expect(screen.getByText('reviewed-test-environment')).toBeTruthy();
  expect(screen.getByText('original-reproduction-criterion')).toBeTruthy();
  expect(screen.getByText('original-revision')).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Open ordinary Build' }).getAttribute('href')).toBe(
    '/builds/reproduction-build',
  );
});

it.each([
  ['missing', 'Lookup scope unavailable'],
  ['cancelled', 'Lookup incomplete; coverage unknown'],
] as const)(
  'distinguishes %s lookup coverage from observed complete capture',
  (capture, expected) => {
    const record = triageCompletion('eligibility');
    record.public_result = {
      accepted: false,
      decision: 'review',
      reason: 'No completeness proof',
      evidence: [],
      ...(capture === 'missing'
        ? {}
        : {
            lookup: {
              status: 'needs_review',
              capture_status: capture,
              coverage: 'unknown',
              observations: [],
            },
          }),
    };
    render(<DiagnosticRecordDetails item={{ kind: 'node_completion', record }} />);
    expect(screen.getByText(expected)).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Systems analyst assessment' })).toBeNull();
  },
);

it('preserves a free-form retained reason even when it equals a finite-choice key', () => {
  const record = triageCompletion('eligibility');
  record.public_result = { accepted: false, decision: 'review', reason: 'critical', evidence: [] };
  render(<DiagnosticRecordDetails item={{ kind: 'node_completion', record }} />);
  expect(screen.getByText('critical')).toBeTruthy();
  expect(screen.queryByText('Critical')).toBeNull();
});
