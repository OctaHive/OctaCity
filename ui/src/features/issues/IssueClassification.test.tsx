// @vitest-environment jsdom
import { cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { ManagementApiError } from '../../api/client';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
import { issuePropertyChoicesFixture } from '../../../tests/fixtures/issues';
import {
  classificationRun,
  issueClassificationFixture,
} from '../../../tests/fixtures/issueClassification';

afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});

it('keeps source-owned built-ins read-only while reviewing an independent Project classification field', async () => {
  const user = userEvent.setup();
  const f = issueClassificationFixture();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue(f.issue);
    api.planningChoices.mockResolvedValue(f.catalog);
    api.classification.mockResolvedValue(f.review);
    api.transitions.mockResolvedValue({
      issue_version: 3,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: {
        source_owned: ['priority', 'component', 'type', 'size'],
        status_mapping_workflow: null,
        push: null,
      },
      items: [],
    });
  });
  const edit = await screen.findByRole('button', { name: 'Edit properties' });
  await waitFor(() => expect((edit as HTMLButtonElement).disabled).toBe(false));
  await user.click(edit);
  for (const label of ['Issue component', 'Issue type', 'Issue size']) {
    expect((screen.getByRole('combobox', { name: label }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  }
  expect(
    (screen.getByRole('button', { name: 'Change priority' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await user.click(screen.getByRole('combobox', { name: 'Delivery risk' }));
  await user.click(screen.getByRole('option', { name: 'High exposure' }));
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2]).toEqual({
    ...f.issue.planning,
    values: { risk: { type: 'choice', value: 'high' } },
  });
  expect(api.edit).not.toHaveBeenCalled();
});

it('keeps committed values readable without presenting unavailable applicability as current', async () => {
  const f = issueClassificationFixture();
  setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue(f.issue);
    api.planningChoices.mockResolvedValue(f.catalog);
    api.classification.mockRejectedValue(
      new ManagementApiError({
        code: 'forbidden',
        message: 'Assessment unavailable.',
        status: 403,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
  });
  const classification = await screen.findByRole('region', { name: 'Classification' });
  expect(await within(classification).findByText('Assessment unavailable.')).toBeTruthy();
  expect(within(classification).queryByText('Current assessment')).toBeNull();
  expect(screen.getByText('Payments')).toBeTruthy();
});

it('refreshes time-dependent evidence without an Issue edit or fabricated verification', async () => {
  const user = userEvent.setup();
  const f = issueClassificationFixture();
  f.review.fields = [
    {
      field: { kind: 'property', id: 'risk', definition_version: 7 },
      problems: [{ code: 'code_subject_unavailable' }],
      requires_verified_evidence: true,
      evidence: [
        {
          record: 'c'.repeat(64),
          problems: [{ code: 'evidence_subject_unavailable', record: 'c'.repeat(64) }],
        },
      ],
    },
  ];
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue(f.issue);
    api.planningChoices.mockResolvedValue(f.catalog);
    api.classification.mockResolvedValue(structuredClone(f.review));
  });
  const classification = await screen.findByRole('region', { name: 'Classification' });
  await within(classification).findByText('Reproduction verification required');
  expect(within(classification).queryByText('Verified reproduction evidence')).toBeNull();
  f.review.fields[0]!.problems.push({ code: 'evidence_expired', record: 'c'.repeat(64) });
  api.classification.mockResolvedValue(f.review);
  await user.click(screen.getByRole('button', { name: 'Refresh task' }));
  expect(await within(classification).findByText('Evidence expired')).toBeTruthy();
  expect(api.classification).toHaveBeenCalledTimes(2);
  expect(api.edit).not.toHaveBeenCalled();
  expect(api.editPlanning).not.toHaveBeenCalled();
});

it('shows committed values and server freshness separately from a conflicting recommendation', async () => {
  const f = issueClassificationFixture();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue(f.issue);
    api.planningChoices.mockResolvedValue(f.catalog);
    api.classification.mockResolvedValue(f.review);
    api.outcomes.mockResolvedValue({
      items: [
        {
          slot: 'triage',
          run_id: classificationRun,
          conclusion: 'Proposed large delivery',
          links: [],
          presentation: {
            kind: 'triage',
            decision: 'eligible',
            recommended_size: 'large',
            classification: { status: 'conflict' },
          },
        },
      ],
    });
  });
  const classification = await screen.findByRole('region', { name: 'Classification' });
  expect(await within(classification).findByText('Requirements changed')).toBeTruthy();
  expect(within(classification).getByText('Manual edit')).toBeTruthy();
  expect(within(classification).getByText('Pipeline assessment')).toBeTruthy();
  expect(
    within(classification).getByRole('link', { name: 'Producing Run' }).getAttribute('href'),
  ).toBe(`/factory/${classificationRun}`);
  expect(
    within(classification)
      .getByRole('link', { name: 'Assessment revision 1' })
      .getAttribute('href'),
  ).toContain('revision=1');
  const properties = screen.getByRole('complementary', { name: 'Properties' });
  expect(within(properties).getByText('Critical incident')).toBeTruthy();
  expect(within(properties).getByText('Small')).toBeTruthy();
  expect(screen.getByText('Classification could not be applied')).toBeTruthy();
  expect(within(classification).queryByText('Large')).toBeNull();
  expect(api.classification).toHaveBeenCalledWith('delivery', issue.id, 3, expect.any(AbortSignal));
  expect(api.editPlanning).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
});

it('edits built-in classification from Project choices without promoting legacy custom properties', async () => {
  const user = userEvent.setup();
  const catalog = issuePropertyChoicesFixture();
  catalog.definitions.definition.components = [
    { id: 'payments', name: 'Payments', retired: false },
  ];
  catalog.definitions.definition.types = [{ id: 'incident', name: 'Incident', retired: false }];
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
  });
  const edit = await screen.findByRole('button', { name: 'Edit properties' });
  await waitFor(() => expect((edit as HTMLButtonElement).disabled).toBe(false));
  await user.click(edit);
  for (const [label, option] of [
    ['Issue component', 'Payments'],
    ['Issue type', 'Incident'],
    ['Issue size', 'Small'],
  ] as const) {
    await user.click(screen.getByRole('combobox', { name: label }));
    await user.click(screen.getByRole('option', { name: option }));
  }
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Issue component: Payments');
  expect(api.editPlanning).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole('button', { name: 'Save properties' }));
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]).toEqual([
    'delivery',
    issue.id,
    {
      definition_version: 7,
      component: 'payments',
      type: 'incident',
      size: 'small',
      assignee: null,
      labels: [],
      due_date: null,
      parent_id: null,
      values: {},
    },
    { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
  ]);
  expect(api.create).not.toHaveBeenCalled();
});
