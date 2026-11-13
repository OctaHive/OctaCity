// @vitest-environment jsdom
import { cleanup, screen, within, waitFor, fireEvent, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { setupExecution } from '../../../tests/fixtures/issueExecutionSupport';
import { ManagementApiError } from '../../api/client';
import { issue } from '../../../tests/fixtures/issueTestSupport';
import { executionFixture } from '../../../tests/fixtures/issueExecutions';
import {
  manuallyPreparedIssue,
  materialSnapshotFixture,
  snapshotConfiguration,
  snapshotFixture,
  classifiedSnapshotFixture,
} from '../../../tests/fixtures/issueExecutionSnapshots';

afterEach(cleanup);
it('keeps the selected execution after opening its assessment revision and returning to Runs', async () => {
  const { executions, api, router } = setupExecution(
    `/projects/delivery/issues/${issue.id}?section=runs&execution=older-request`,
    9,
  );
  const snapshot = classifiedSnapshotFixture();
  const receipt = executionFixture('admitted', 'older-request');
  receipt.intent.expected_version = 6;
  receipt.intent.target = snapshot.target;
  receipt.snapshot_digest = 'f'.repeat(64);
  executions.get.mockResolvedValue(receipt);
  executions.snapshot.mockResolvedValue({
    availability: 'available',
    digest: receipt.snapshot_digest,
    snapshot,
    subject: null,
  });
  api.revision.mockResolvedValue({
    ...issue,
    version: 5,
    content: { ...issue.content, description: 'Original assessed requirements' },
  });
  const original = await screen.findByRole('region', { name: 'Frozen execution input' });
  await userEvent.click(
    await within(original).findByRole('link', { name: 'Assessment revision 5' }),
  );
  expect(await screen.findByText('Original assessed requirements')).toBeTruthy();
  await userEvent.click(screen.getByRole('link', { name: 'Runs' }));
  const retained = await screen.findByRole('region', { name: 'Frozen execution input' });
  expect(await within(retained).findByText('Routine delivery · routine')).toBeTruthy();
  expect(within(retained).getByRole('link', { name: 'Assessment revision 5' })).toBeTruthy();
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('older-request');
  expect(executions.snapshot).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    'older-request',
    expect.any(AbortSignal),
  );
  expect(executions.review).not.toHaveBeenCalled();
  expect(executions.start).not.toHaveBeenCalled();
});

it('retains the exact external mapping and observation in snapshot attribution', async () => {
  const snapshot = classifiedSnapshotFixture();
  snapshot.attribution.exposure!.origin = {
    kind: 'source',
    actor: { kind: 'unauthenticated_management', identity: null },
    mapping: { identity: 'provider-classification', version: 'v4', digest: 'd'.repeat(64) },
    observation: 'e'.repeat(64),
    native_correlation: '8'.repeat(64),
  };
  const { executions, getConfiguration } = setupExecution(undefined, 6);
  const configuration = snapshotConfiguration();
  configuration.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(configuration);
  executions.review.mockResolvedValue({
    version: 6,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('External source')).toBeTruthy();
  expect(
    within(dialog).getByText(`Source mapping: provider-classification · v4 · ${'d'.repeat(64)}`),
  ).toBeTruthy();
  expect(within(dialog).getByText(`Observation record: ${'e'.repeat(64)}`)).toBeTruthy();
  expect(within(dialog).queryByRole('link', { name: 'Producing Run' })).toBeNull();
  expect(executions.start).not.toHaveBeenCalled();
});

it.each(['preview', 'detail'])(
  'explains frozen classification attribution in %s without substituting current assessments',
  async (mode) => {
    const snapshot = classifiedSnapshotFixture();
    const current = manuallyPreparedIssue();
    current.version = mode === 'preview' ? 6 : 9;
    current.classification = {
      fields: [
        {
          ...snapshot.attribution.priority!,
          assessed_version: current.version,
          requirements_version: current.version,
          field: { kind: 'priority', definition_version: 9 },
        },
      ],
    };
    const { executions, getConfiguration } = setupExecution(
      mode === 'detail'
        ? `/projects/delivery/issues/${issue.id}?section=runs&execution=request-one`
        : undefined,
      current.version,
      current,
    );
    let view: HTMLElement;
    if (mode === 'preview') {
      const configuration = snapshotConfiguration();
      configuration.configuration.flow.issue_snapshot = snapshot.input;
      getConfiguration.mockResolvedValue(configuration);
      executions.review.mockResolvedValue({
        version: 6,
        target: snapshot.target,
        snapshot,
        snapshot_digest: 'f'.repeat(64),
      });
      await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
      await screen.findByRole('combobox', { name: 'Factory Configurations' });
      await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
      view = await screen.findByRole('dialog');
    } else {
      const receipt = executionFixture();
      receipt.intent.expected_version = 6;
      receipt.intent.target = snapshot.target;
      receipt.snapshot_digest = 'f'.repeat(64);
      executions.get.mockResolvedValue(receipt);
      executions.snapshot.mockResolvedValue({
        availability: 'available',
        digest: receipt.snapshot_digest,
        snapshot,
        subject: null,
      });
      view = await screen.findByRole('region', { name: 'Frozen execution input' });
    }
    expect(await within(view).findByText('Pipeline assessment')).toBeTruthy();
    expect(within(view).getByText('Manual edit')).toBeTruthy();
    expect(within(view).getByText('Project field definitions: v7')).toBeTruthy();
    expect(within(view).getAllByText('Assessment field definitions: v2')).toHaveLength(2);
    expect(within(view).getByText('Snapshot requirements revision: 3')).toBeTruthy();
    expect(within(view).getAllByText('Assessment requirements revision: 3')).toHaveLength(2);
    for (const version of [4, 5]) {
      const link = within(view).getByRole('link', { name: `Assessment revision ${version}` });
      const destination = new URL(link.getAttribute('href')!, 'http://console.test');
      expect(destination.pathname).toBe(`/projects/delivery/issues/${issue.id}`);
      expect(destination.searchParams.get('section')).toBe('activity');
      expect(destination.searchParams.get('revision')).toBe(String(version));
      expect(destination.searchParams.get('execution')).toBe(
        mode === 'detail' ? 'request-one' : null,
      );
    }
    expect(within(view).getByRole('link', { name: 'Producing Run' }).getAttribute('href')).toBe(
      '/factory/44000000-0000-4000-8000-000000000001',
    );
    expect(view.querySelector('time[datetime="2023-11-14T22:13:21.000Z"]')).not.toBeNull();
    expect(within(view).getByText('c'.repeat(64))).toBeTruthy();
    expect(within(view).getByText('No evidence references in this assessment.')).toBeTruthy();
    expect(within(view).getByText('Not assessed')).toBeTruthy();
    expect(within(view).queryByText('Verified reproduction evidence')).toBeNull();
    expect(within(view).queryByText('Assessment field definitions: v9')).toBeNull();
    expect(executions.start).not.toHaveBeenCalled();
  },
);

it('reviews server-resolved fields and optional omissions for a manually prepared Issue without a producer', async () => {
  const { executions, getConfiguration } = setupExecution(undefined, 1, manuallyPreparedIssue());
  const snapshot = snapshotFixture();
  getConfiguration.mockResolvedValue(snapshotConfiguration());
  executions.review.mockResolvedValue({
    version: 1,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(await screen.findByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('high')).toBeTruthy();
  expect(within(dialog).getByText('small')).toBeTruthy();
  expect(within(dialog).getByText(/Optional input omitted.*component.*missing/i)).toBeTruthy();
  expect(screen.queryByRole('checkbox', { name: 'Use a result from an earlier run' })).toBeNull();
  expect(screen.queryByRole('spinbutton', { name: 'Work priority' })).toBeNull();
  expect(executions.list).not.toHaveBeenCalled();
  expect(executions.start).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' }));
  await within(dialog).findByRole('link', { name: 'Open Run' });
  expect(executions.start.mock.calls[0]?.[2]).toEqual({
    expected_snapshot_digest: 'f'.repeat(64),
    target: expect.objectContaining({ mapping: 'title_description_acceptance_v1' }),
  });
});

it('previews exact selected comment and both document owners as plain data with limits and explicit omissions', async () => {
  const { executions, getConfiguration } = setupExecution();
  const snapshot = materialSnapshotFixture();
  const details = snapshotConfiguration();
  details.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(details);
  executions.review.mockResolvedValue({
    version: 1,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(await screen.findByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  const comment = await within(dialog).findByRole('region', { name: 'Comment · comment-approved' });
  expect(within(comment).getByText('Approved comment <script>alert(1)</script>')).toBeTruthy();
  expect(comment.textContent).toContain('1'.repeat(64));
  expect(comment.querySelector('script')).toBeNull();
  const document = within(dialog).getByRole('region', {
    name: 'Issue document · Task investigation',
  });
  expect(within(document).getByText('Original task document')).toBeTruthy();
  expect(document.textContent).toContain('issue-document-v1');
  expect(document.textContent).toContain(snapshot.issue_id);
  expect(
    within(dialog).getByRole('region', { name: 'Project document · Shared instructions' })
      .textContent,
  ).toContain('shared-document-v4');
  expect(
    within(dialog).getByText('context · Required · 3/4 entries · 1000 bytes maximum'),
  ).toBeTruthy();
  expect(within(dialog).getByText('Optional input omitted: extra · unavailable')).toBeTruthy();
  expect(executions.start).not.toHaveBeenCalled();
});

it('requires a new server preview after a material-selection conflict while preserving the repository draft', async () => {
  const { executions, api, getConfiguration } = setupExecution();
  const snapshot = materialSnapshotFixture();
  const details = snapshotConfiguration();
  details.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(details);
  executions.review.mockImplementation(async (_p, _i, request) => ({
    version: 1,
    target: request.target,
    snapshot: { ...snapshot, target: request.target },
    snapshot_digest: 'f'.repeat(64),
  }));
  executions.start.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      status: 409,
      message: 'Issue material selection changed',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source' }));
  await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Exact revision' }), 'preserved-sha');
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await within(await screen.findByRole('dialog')).findByText('Original task document');
  await userEvent.click(screen.getByRole('button', { name: 'Confirm pipeline launch' }));
  await screen.findByText('Issue material selection changed');
  const newer = structuredClone(snapshot);
  newer.version = 2;
  newer.values.context = ['Replacement approved text'];
  executions.review.mockImplementation(async (_p, _i, request) => ({
    version: 2,
    target: request.target,
    snapshot: { ...newer, target: request.target },
    snapshot_digest: '9'.repeat(64),
  }));
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, acceptance: 'Check checkout works' },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  expect(await screen.findByRole('textbox', { name: 'Exact revision' })).toHaveProperty(
    'value',
    'preserved-sha',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('Replacement approved text')).toBeTruthy();
  expect(within(dialog).queryByText('Original task document')).toBeNull();
  expect(executions.start).toHaveBeenCalledTimes(1);
  expect(executions.review.mock.calls.at(-1)?.[3]).toBe(2);
});

it('keeps field-specific blockers visible and cannot confirm a failed snapshot review', async () => {
  const { executions, getConfiguration } = setupExecution();
  getConfiguration.mockResolvedValue(snapshotConfiguration());
  executions.review.mockRejectedValue(
    new ManagementApiError({
      code: 'invalid_request',
      status: 400,
      message: 'Input prerequisites are not met',
      requestId: null,
      retryAfterMilliseconds: null,
      validationIssues: [
        { field: '/issue_snapshot/materials/context', code: 'issue_snapshot_material_unavailable' },
      ],
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('/issue_snapshot/materials/context')).toBeTruthy();
  expect(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it('reads the original execution snapshot rather than replacing it with current Issue values or newer text', async () => {
  const { executions, api } = setupExecution(
    `/projects/delivery/issues/${issue.id}?section=runs&execution=request-one`,
    2,
  );
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: {
      ...issue.content,
      priority: 'low',
      description: 'New task document exists',
      acceptance: 'Current acceptance',
    },
  });
  const receipt = executionFixture();
  receipt.snapshot_digest = 'f'.repeat(64);
  receipt.intent.target = snapshotFixture().target;
  executions.get.mockResolvedValue(receipt);
  executions.snapshot.mockResolvedValue({
    availability: 'available',
    digest: receipt.snapshot_digest,
    snapshot: materialSnapshotFixture(),
    subject: null,
  });
  const input = await screen.findByRole('region', { name: 'Frozen execution input' });
  expect(await within(input).findByText('Original task document')).toBeTruthy();
  expect(within(input).getByText('high')).toBeTruthy();
  expect(within(input).queryByText('low')).toBeNull();
  expect(within(input).queryByText('New task document exists')).toBeNull();
  expect(
    within(input).getByText(
      'Current Issue version: 2. This execution retains its original fields and selected text.',
    ),
  ).toBeTruthy();
  expect(executions.snapshot).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'request-one',
    expect.any(AbortSignal),
  );
  expect(executions.review).not.toHaveBeenCalled();
  expect(executions.start).not.toHaveBeenCalled();
});

it('does not offer a producer Run for a target without an explicit producer contract', async () => {
  const { executions } = setupExecution();
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  expect(screen.queryByRole('checkbox', { name: 'Use a result from an earlier run' })).toBeNull();
  expect(screen.queryByRole('combobox', { name: 'Earlier Issue run' })).toBeNull();
  expect(executions.list).not.toHaveBeenCalled();
});

it('removes previously displayed text when current body access is revoked on refresh', async () => {
  const { executions } = setupExecution(
    `/projects/delivery/issues/${issue.id}?section=runs&execution=request-one`,
  );
  const receipt = executionFixture();
  receipt.snapshot_digest = 'f'.repeat(64);
  receipt.intent.target = snapshotFixture().target;
  executions.get.mockResolvedValue(receipt);
  executions.snapshot.mockResolvedValue({
    availability: 'available',
    digest: receipt.snapshot_digest,
    snapshot: materialSnapshotFixture(),
    subject: null,
  });
  const input = await screen.findByRole('region', { name: 'Frozen execution input' });
  await within(input).findByText('Original task document');
  executions.snapshot.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Body access revoked',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  // Use the same public refresh action as an operator, without replacing the selected request.
  await userEvent.click(within(input).getByRole('button', { name: 'Refresh frozen input' }));
  await within(input).findByText('Body access revoked');
  expect(within(input).queryByText('Original task document')).toBeNull();
  expect(executions.review).not.toHaveBeenCalled();
});

it('refuses a retained snapshot returned for another Issue even if its envelope digest matches', async () => {
  const { executions } = setupExecution(
    `/projects/delivery/issues/${issue.id}?section=runs&execution=request-one`,
  );
  const receipt = executionFixture();
  receipt.snapshot_digest = 'f'.repeat(64);
  receipt.intent.target = snapshotFixture().target;
  executions.get.mockResolvedValue(receipt);
  executions.snapshot.mockResolvedValue({
    availability: 'available',
    digest: receipt.snapshot_digest,
    snapshot: { ...materialSnapshotFixture(), issue_id: 'another-issue' },
    subject: null,
  });
  const input = await screen.findByRole('region', { name: 'Frozen execution input' });
  expect(
    await within(input).findByText(
      'The original input is unavailable under current access or retention. Current data is not substituted.',
    ),
  ).toBeTruthy();
  expect(within(input).queryByText('Original task document')).toBeNull();
});

it('cannot confirm a preview for another target', async () => {
  const { executions, getConfiguration } = setupExecution();
  getConfiguration.mockResolvedValue(snapshotConfiguration());
  const snapshot = snapshotFixture();
  executions.review.mockResolvedValue({
    version: 1,
    target: { ...snapshot.target, configuration_id: 'another-target' },
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  await within(dialog).findByText(
    'The preview does not match the reviewed Issue and target. Close it and review again.',
  );
  expect(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(within(dialog).queryByText('high')).toBeNull();
});

it('can abandon a pending preview without losing the source draft or reviving its late response', async () => {
  const { executions, getConfiguration } = setupExecution();
  getConfiguration.mockResolvedValue(snapshotConfiguration());
  let finish: (() => void) | undefined;
  const snapshot = snapshotFixture();
  executions.review.mockImplementationOnce(async (_p, _i, request) => {
    await new Promise<void>((resolve) => {
      finish = resolve;
    });
    return {
      version: 1,
      target: request.target,
      snapshot: { ...snapshot, target: request.target },
      snapshot_digest: 'f'.repeat(64),
    };
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source' }));
  await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Exact revision' }), 'preserved-sha');
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Cancel input review' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  finish?.();
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByRole('textbox', { name: 'Exact revision' })).toHaveProperty(
    'value',
    'preserved-sha',
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it('explains a selected ordinary Project field using the frozen definition and choice name', async () => {
  const { executions, getConfiguration } = setupExecution();
  const snapshot = snapshotFixture();
  snapshot.input.definition_version = 7;
  snapshot.input.fields.environment = {
    required: true,
    source: { kind: 'property', id: 'deployment_env' },
  };
  snapshot.values.environment = 'stage';
  snapshot.definitions['7'] = {
    selected_v1: {
      project_id: 'delivery',
      version: 7,
      publication_digest: '7'.repeat(64),
      published_at: 1,
      properties: [
        {
          definition: {
            id: 'deployment_env',
            name: 'Deployment environment',
            retired: false,
            kind: {
              type: 'choice',
              choices: [{ id: 'stage', name: 'Staging cluster', retired: false }],
            },
          },
        },
      ],
    },
  };
  const details = snapshotConfiguration();
  details.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(details);
  executions.review.mockResolvedValue({
    version: 1,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('Deployment environment · deployment_env')).toBeTruthy();
  expect(within(dialog).getByText('Staging cluster · stage')).toBeTruthy();
  expect(within(dialog).getByText('Project field definitions: v7')).toBeTruthy();
});

it('keeps exact optional source identities visible when their bodies are omitted', async () => {
  const { executions, getConfiguration } = setupExecution();
  const snapshot = materialSnapshotFixture();
  const details = snapshotConfiguration();
  details.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(details);
  executions.review.mockResolvedValue({
    version: 1,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText(`expired-comment · ${'4'.repeat(64)}`)).toBeTruthy();
  expect(within(dialog).getByText('Optional input omitted: extra · unavailable')).toBeTruthy();
});

it('hides a cached review body when post-command refresh loses independent text authority', async () => {
  const { executions, getConfiguration } = setupExecution();
  const snapshot = materialSnapshotFixture();
  const details = snapshotConfiguration();
  details.configuration.flow.issue_snapshot = snapshot.input;
  getConfiguration.mockResolvedValue(details);
  executions.review.mockResolvedValue({
    version: 1,
    target: snapshot.target,
    snapshot,
    snapshot_digest: 'f'.repeat(64),
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  const dialog = await screen.findByRole('dialog');
  await within(dialog).findByText('Original task document');
  executions.review.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Review body access revoked',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' }));
  await within(dialog).findByText('Review body access revoked');
  expect(within(dialog).queryByText('Original task document')).toBeNull();
});

it('ignores an invalid legacy scheduling draft after switching to a policy-driven snapshot target', async () => {
  const { executions, getConfiguration, listConfigurations, configuration } = setupExecution();
  const snapshot = snapshotConfiguration();
  snapshot.configuration.reference.id = 'snapshot-config';
  const legacy = {
    ...snapshot,
    configuration: {
      ...snapshot.configuration,
      reference: configuration,
      flow: { ...snapshot.configuration.flow, issue_snapshot: undefined },
    },
  };
  getConfiguration.mockImplementation(async (id) => (id === 'snapshot-config' ? snapshot : legacy));
  listConfigurations.mockResolvedValue({
    items: [configuration, snapshot.configuration.reference].map((reference) => ({
      ...reference,
      enabled: true,
      created_at_unix_ms: 1,
      published_at_unix_ms: 1,
    })),
    total: 2,
    next_cursor: null,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  fireEvent.change(await screen.findByRole('spinbutton', { name: 'Work priority' }), {
    target: { value: '1000000000' },
  });
  await userEvent.click(screen.getByRole('combobox', { name: 'Factory Configurations' }));
  await userEvent.click(screen.getByRole('option', { name: /snapshot-config/ }));
  await waitFor(() =>
    expect(screen.queryByRole('spinbutton', { name: 'Work priority' })).toBeNull(),
  );
  expect(screen.getByRole('button', { name: 'Review pipeline launch' })).toHaveProperty(
    'disabled',
    false,
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it('explains renewed review when the Issue changes while its input preview is loading', async () => {
  const { executions, api, getConfiguration } = setupExecution();
  const snapshot = snapshotFixture();
  getConfiguration.mockResolvedValue(snapshotConfiguration());
  let finish: (() => void) | undefined;
  executions.review.mockImplementationOnce(async (_p, _i, request) => {
    await new Promise<void>((resolve) => {
      finish = resolve;
    });
    return {
      version: 1,
      target: request.target,
      snapshot: { ...snapshot, target: request.target },
      snapshot_digest: 'f'.repeat(64),
    };
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
  await screen.findByRole('combobox', { name: 'Factory Configurations' });
  await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
  await screen.findByRole('button', { name: 'Cancel input review' });
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, acceptance: 'Changed selected input' },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Refresh task' })).toHaveProperty('disabled', false),
  );
  await act(async () => {
    finish?.();
  });
  const dialog = await screen.findByRole('dialog');
  expect(
    await within(dialog).findByText(
      'The Issue changed after this review. Close the preview and review the current inputs again.',
    ),
  ).toBeTruthy();
  expect(within(dialog).getByRole('button', { name: 'Confirm pipeline launch' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it.each(['selected_v1', 'legacy'])(
  'reviews frozen semantic property and choice names from %s definitions',
  async (format) => {
    const { executions, getConfiguration } = setupExecution();
    const snapshot = snapshotFixture();
    snapshot.input.definition_version = 7;
    snapshot.input.fields.exposure = {
      required: true,
      source: { kind: 'classification', slot: 'risk' },
    };
    snapshot.values.exposure = 'routine';
    const property = {
      id: 'risk_band',
      name: 'Delivery exposure',
      retired: false,
      kind: {
        type: 'choice' as const,
        choices: [{ id: 'routine', name: 'Routine delivery', retired: false }],
      },
    };
    snapshot.definitions['7'] =
      format === 'selected_v1'
        ? {
            selected_v1: {
              project_id: 'delivery',
              version: 7,
              published_at: 1,
              publication_digest: '7'.repeat(64),
              properties: [{ classification_slot: 'risk', definition: property }],
            },
          }
        : {
            project_id: 'delivery',
            version: 7,
            published_at: 1,
            definition: {
              labels: [],
              properties: [property],
              classification: { risk: 'risk_band' },
            },
          };
    const details = snapshotConfiguration();
    details.configuration.flow.issue_snapshot = snapshot.input;
    getConfiguration.mockResolvedValue(details);
    executions.review.mockResolvedValue({
      version: 1,
      target: snapshot.target,
      snapshot,
      snapshot_digest: 'f'.repeat(64),
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Start pipeline' }));
    await screen.findByRole('combobox', { name: 'Factory Configurations' });
    await userEvent.click(screen.getByRole('button', { name: 'Review pipeline launch' }));
    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByText('Delivery exposure · risk_band')).toBeTruthy();
    expect(within(dialog).getByText('Routine delivery · routine')).toBeTruthy();
    expect(executions.start).not.toHaveBeenCalled();
  },
);
