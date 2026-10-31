// @vitest-environment jsdom
import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';

import { projectDocumentsApi } from '../../api/projectDocuments';
import { ManagementApiError } from '../../api/client';
import { manualFlowEditorFixture } from '../../../tests/fixtures/manualFlowEditor';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('reviews new process criteria and explicitly selects recipients without changing Project goals or ticket acceptance', async () => {
  const { draft, documents } = manualFlowEditorFixture();
  const published = {
    project_id: 'delivery',
    name: 'Pipeline completion criteria',
    artifact: {
      artifact_id: '11111111-1111-4111-8111-111111111111',
      content_digest: 'b'.repeat(64),
      encoded_size: 32,
    },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  };
  const publish = vi.spyOn(projectDocumentsApi, 'publish').mockResolvedValue(published);
  const purpose = within(await screen.findByRole('group', { name: 'Pipeline purpose' }));
  await userEvent.click(purpose.getByRole('button', { name: 'Edit completion criteria' }));
  await userEvent.type(
    purpose.getByRole('textbox', { name: 'Completion criteria' }),
    'Retain an explicit reviewed outcome.',
  );
  await userEvent.click(purpose.getByRole('button', { name: 'Review criteria publication' }));
  expect(publish).not.toHaveBeenCalled();
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText(/Retain an explicit reviewed outcome\./)).toBeTruthy();
  expect(dialog.getByText(/The Flow is published separately\./)).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Save criteria input' }));
  expect(
    await dialog.findByText(
      'Input saved in this draft. Select its recipient steps, then review the Flow publication.',
    ),
  ).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  await userEvent.click(purpose.getByRole('checkbox', { name: 'Criteria recipient · assess' }));
  expect(draft().root.process?.criteria).toEqual({
    artifact: published.artifact,
    recipients: ['assess'],
  });
  expect(draft().root.process?.goals).toBeUndefined();
  expect(purpose.getByRole('checkbox', { name: 'Criteria recipient · classify' })).toHaveProperty(
    'checked',
    false,
  );
  expect(documents.map((document) => document.name)).toEqual(['Goals', 'Components', 'Priorities']);
  expect(publish).toHaveBeenCalledWith(
    'delivery',
    {
      name: 'Pipeline completion criteria',
      content: 'Retain an explicit reviewed outcome.',
    },
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
});

it('opens exact retained goals for editing and publishes changed content without repinning criteria or changing recipients', async () => {
  const goals = { artifact_id: 'goals-v1', content_digest: 'a'.repeat(64), encoded_size: 20 };
  const criteria = {
    artifact_id: 'priorities-v1',
    content_digest: 'a'.repeat(64),
    encoded_size: 20,
  };
  const { draft, read } = manualFlowEditorFixture((value) => {
    value.root.process = {
      goals: { artifact: goals, recipients: ['assess'] },
      criteria: { artifact: criteria, recipients: ['classify'] },
    };
  });
  const publish = vi.spyOn(projectDocumentsApi, 'publish').mockResolvedValue({
    project_id: 'delivery',
    name: 'Pipeline goals',
    published_at: 2,
    expires_at: Date.now() + 60_000,
    artifact: {
      ...goals,
      artifact_id: '22222222-2222-4222-8222-222222222222',
      content_digest: 'c'.repeat(64),
    },
  });
  expect(read).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Edit pipeline goals' }));
  const content = await screen.findByRole('textbox', { name: 'Pipeline goals' });
  expect(await screen.findByDisplayValue('Exact goals-v1 content')).toBeTruthy();
  expect(read).toHaveBeenCalledWith('delivery', 'goals-v1');
  await userEvent.type(content, '\nClarify the next action.');
  await userEvent.click(screen.getByRole('button', { name: 'Review goals publication' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Save goals input' }));
  expect(
    await dialog.findByText(
      'Input saved in this draft. Select its recipient steps, then review the Flow publication.',
    ),
  ).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  expect(draft().root.process?.criteria).toEqual({ artifact: criteria, recipients: ['classify'] });
  expect(draft().root.process?.goals?.recipients).toEqual(['assess']);
  expect(draft().root.process?.goals?.artifact.content_digest).toBe('c'.repeat(64));
  expect(publish.mock.calls[0]?.[1].content).toBe(
    'Exact goals-v1 content\nClarify the next action.',
  );
});

it('detaches completion criteria in the draft without deleting inputs or changing pipeline goals', async () => {
  const goals = {
    artifact: { artifact_id: 'goals-v1', content_digest: 'a'.repeat(64), encoded_size: 20 },
    recipients: ['assess'],
  };
  const { draft, read } = manualFlowEditorFixture((value) => {
    value.root.process = {
      goals,
      criteria: {
        artifact: { ...goals.artifact, artifact_id: 'priorities-v1' },
        recipients: ['classify'],
      },
    };
  });
  const publish = vi.spyOn(projectDocumentsApi, 'publish');
  await userEvent.click(screen.getByRole('button', { name: 'Detach completion criteria' }));
  expect(draft().root.process?.criteria).toBeUndefined();
  expect(draft().root.process?.goals).toEqual(goals);
  expect(read).not.toHaveBeenCalled();
  expect(publish).not.toHaveBeenCalled();
});

it('keeps process recipients attached to the same logical step when it is renamed or removed', async () => {
  const { draft } = manualFlowEditorFixture((value) => {
    value.root.process = {
      criteria: {
        artifact: {
          artifact_id: 'priorities-v1',
          content_digest: 'a'.repeat(64),
          encoded_size: 20,
        },
        recipients: ['assess'],
      },
    };
  });
  const name = screen.getByRole('textbox', { name: 'Node name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'inspect');
  expect(draft().root.process?.criteria?.recipients).toEqual(['inspect']);
  expect(screen.getByRole('checkbox', { name: 'Criteria recipient · inspect' })).toHaveProperty(
    'checked',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Remove node' }));
  expect(draft().root.process?.criteria?.recipients).toEqual([]);
  expect(draft().root.nodes.map((node) => node.key)).toEqual(['classify']);
});

it('keeps process-owned criteria separate from explicitly selected Project documents in a saved Flow', async () => {
  const artifact = { artifact_id: 'goals-v1', content_digest: 'a'.repeat(64), encoded_size: 20 };
  const { draft } = manualFlowEditorFixture((value) => {
    value.root.process = { criteria: { artifact, recipients: ['assess'] } };
    value.root.nodes[0]!.context_binding = {
      entries: {
        'process.criteria': {
          source_kind: 'specification',
          source: {
            value: {
              mapping: 'constant',
              value: { kind: 'artifact', value: artifact },
            },
          },
        },
      },
    };
  });
  const documents = within(screen.getByRole('group', { name: 'Documents' }));
  const goals = await documents.findByRole('checkbox', { name: 'Goals' });
  expect(goals).toHaveProperty('checked', false);
  expect(documents.queryByRole('listitem', { name: 'Goals' })).toBeNull();
  await userEvent.click(goals);
  expect(Object.keys(draft().root.nodes[0]!.context_binding!.entries)).toHaveLength(2);
  expect(draft().root.process?.criteria).toEqual({ artifact, recipients: ['assess'] });
});

it('replays an unknown criteria write with the same intent and attaches only its confirmed input', async () => {
  const { draft } = manualFlowEditorFixture();
  const publication = {
    project_id: 'delivery',
    name: 'Pipeline completion criteria',
    published_at: 1,
    expires_at: Date.now() + 60_000,
    artifact: {
      artifact_id: '11111111-1111-4111-8111-111111111111',
      content_digest: 'b'.repeat(64),
      encoded_size: 32,
    },
  };
  const publish = vi
    .spyOn(projectDocumentsApi, 'publish')
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Reply unavailable',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValueOnce(publication);
  await userEvent.click(screen.getByRole('button', { name: 'Edit completion criteria' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Completion criteria' }),
    'Retain the reviewed result.',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review criteria publication' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Save criteria input' }));
  await dialog.findByRole('button', { name: 'Retry same command' });
  expect(draft().root.process).toBeUndefined();
  await userEvent.click(dialog.getByRole('button', { name: 'Retry same command' }));
  expect(await dialog.findByText(/Input saved in this draft/)).toBeTruthy();
  expect(publish.mock.calls[0]).toEqual(publish.mock.calls[1]);
  expect(draft().root.process?.criteria).toEqual({
    artifact: publication.artifact,
    recipients: [],
  });
});

it('keeps exact process input identifiers in optional details without reading private content', async () => {
  const { read } = manualFlowEditorFixture((value) => {
    value.root.process = {
      criteria: {
        artifact: {
          artifact_id: 'priorities-v1',
          content_digest: 'a'.repeat(64),
          encoded_size: 20,
        },
        recipients: ['classify'],
      },
    };
  });
  const purpose = within(screen.getByRole('group', { name: 'Pipeline purpose' }));
  await userEvent.click(purpose.getByText('Input details'));
  expect(purpose.getByText('priorities-v1')).toBeTruthy();
  expect(purpose.getByText('a'.repeat(64))).toBeTruthy();
  expect(read).not.toHaveBeenCalled();
});

it.each(['forbidden', 'not_found'] as const)(
  'preserves the selected input after a %s criteria read and blocks empty replacement',
  async (code) => {
    const criteria = {
      artifact: { artifact_id: 'priorities-v1', content_digest: 'a'.repeat(64), encoded_size: 20 },
      recipients: ['classify'],
    };
    const { draft, read } = manualFlowEditorFixture((value) => {
      value.root.process = { criteria };
    });
    read.mockRejectedValue(
      new ManagementApiError({
        code,
        message: 'Input unavailable',
        requestId: 'criteria-read',
        status: code === 'forbidden' ? 403 : 404,
        retryAfterMilliseconds: null,
      }),
    );
    const publish = vi.spyOn(projectDocumentsApi, 'publish');
    await userEvent.click(screen.getByRole('button', { name: 'Edit completion criteria' }));
    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'Completion criteria' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(screen.getByRole('button', { name: 'Review criteria publication' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(draft().root.process?.criteria).toEqual(criteria);
    expect(publish).not.toHaveBeenCalled();
  },
);
