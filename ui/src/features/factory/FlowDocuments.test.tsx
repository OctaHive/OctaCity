// @vitest-environment jsdom
import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';

import { projectDocumentsApi } from '../../api/projectDocuments';
import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { manualFlowEditorFixture as fixture } from '../../../tests/fixtures/manualFlowEditor';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('selects exact Project documents with independent instruction and reference roles per step', async () => {
  const { draft, documents, read } = fixture();
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  await userEvent.click(section.getByRole('checkbox', { name: 'Components' }));
  await userEvent.click(section.getByRole('checkbox', { name: 'Priorities' }));
  await userEvent.click(section.getByRole('combobox', { name: 'Document role · Goals' }));
  await userEvent.click(screen.getByRole('option', { name: 'Agent instructions' }));
  const selected = draft().root.nodes[0]!.context_binding!.entries;
  expect(
    Object.values(selected)
      .map((entry) => entry.source_kind)
      .sort(),
  ).toEqual(['policy', 'specification', 'specification']);
  expect(JSON.stringify(selected)).toContain(documents[0]!.artifact.content_digest);
  expect(draft().root.nodes[1]!.context_binding).toBeNull();
  expect(read).not.toHaveBeenCalled();
  await userEvent.click(section.getByRole('button', { name: 'Remove document Priorities' }));
  expect(Object.keys(draft().root.nodes[0]!.context_binding!.entries)).toHaveLength(2);
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: classify' }));
  const sibling = within(screen.getByRole('group', { name: 'Documents' }));
  expect(sibling.getByRole('checkbox', { name: 'Goals' })).toHaveProperty('checked', false);
  await userEvent.click(sibling.getByRole('checkbox', { name: 'Goals' }));
  expect(Object.keys(draft().root.nodes[1]!.context_binding!.entries)).toHaveLength(1);
  expect(Object.keys(draft().root.nodes[0]!.context_binding!.entries)).toHaveLength(2);
  expect(screen.queryByRole('combobox', { name: 'Project rules' })).toBeNull();
  expect(screen.queryByRole('textbox', { name: /alias/i })).toBeNull();
});

it('previews only the explicitly requested exact document without attaching extra inputs', async () => {
  const { read, draft } = fixture();
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  expect(read).not.toHaveBeenCalled();
  const goals = within(section.getByRole('listitem', { name: 'Goals' }));
  await userEvent.click(goals.getByRole('button', { name: 'Preview document' }));
  expect(await screen.findByText('Exact goals-v1 content')).toBeTruthy();
  expect(read).toHaveBeenCalledWith('delivery', 'goals-v1');
  expect(Object.keys(draft().root.nodes[0]!.context_binding!.entries)).toHaveLength(1);
  expect(draft().root.nodes[1]!.context_binding).toBeNull();
});

it('opens Project document management and returns with the exact pipeline draft and independent selections intact', async () => {
  const { draft, documents, list } = fixture();
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  const before = structuredClone(draft());
  const extra = {
    ...documents[0]!,
    name: 'Terminology',
    artifact: { ...documents[0]!.artifact, artifact_id: 'terms-v1' },
  };
  const publish = vi.spyOn(projectDocumentsApi, 'publish').mockImplementation(async () => {
    list.mockResolvedValue({ items: [...documents, extra], next_cursor: null });
    return extra;
  });
  await userEvent.click(section.getByRole('button', { name: 'Manage Project documents' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Terminology');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Document text' }),
    'Project terminology',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review document publication' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Publish document' }));
  expect(await dialog.findByText(/Document published/)).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  await userEvent.click(screen.getByRole('button', { name: 'Return to step' }));
  expect(draft()).toEqual(before);
  expect(await screen.findByRole('checkbox', { name: 'Terminology' })).toHaveProperty(
    'checked',
    false,
  );
  expect(screen.getByRole('checkbox', { name: 'Goals' })).toHaveProperty('checked', true);
  expect(publish).toHaveBeenCalledTimes(1);
});

it('edits documents in one list without duplicate technical Context entry editors', async () => {
  fixture();
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  expect(screen.queryByRole('textbox', { name: 'Context entry name' })).toBeNull();
});

it('removing the last document preserves an explicit candidate revision for validation instead of retargeting the step', async () => {
  const candidate = { value: { mapping: 'pointer' as const, path: '/reviewed-candidate' } };
  const { draft } = fixture((value) => {
    value.root.nodes[0]!.context_binding = { entries: {}, candidate_revision: candidate };
  });
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  await userEvent.click(section.getByRole('button', { name: 'Remove document Goals' }));
  expect(draft().root.nodes[0]!.context_binding?.candidate_revision).toEqual(candidate);
});

it('changing a document role preserves its declared presence condition and exact binding identity', async () => {
  const { draft } = fixture((value) => {
    value.root.nodes[0]!.context_binding = {
      entries: {
        'reviewed-guidance': {
          when_present: '/approved-guidance',
          source_kind: 'specification',
          source: {
            value: {
              mapping: 'constant',
              value: {
                kind: 'artifact',
                value: {
                  artifact_id: 'goals-v1',
                  content_digest: 'a'.repeat(64),
                  encoded_size: 20,
                },
              },
            },
          },
        },
      },
    };
  });
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await section.findByRole('checkbox', { name: 'Goals' });
  await userEvent.click(section.getByRole('combobox', { name: 'Document role · Goals' }));
  await userEvent.click(screen.getByRole('option', { name: 'Agent instructions' }));
  expect(draft().root.nodes[0]!.context_binding!.entries['reviewed-guidance']?.when_present).toBe(
    '/approved-guidance',
  );
  expect(draft().root.nodes[0]!.context_binding!.entries['reviewed-guidance']?.source_kind).toBe(
    'policy',
  );
});

it('explains an explicit Profile input prerequisite and lets the operator bind a document without typing an alias', async () => {
  const binding =
    factoryProfileConfigurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!
      .profile_selection!;
  binding.candidates.default!.profile.definition.context_requirements = ['governing-guidance'];
  const { draft } = fixture((value) => {
    value.root.nodes[0]!.profile_selection = binding;
  });
  const section = within(await screen.findByRole('group', { name: 'Documents' }));
  await userEvent.click(await section.findByRole('checkbox', { name: 'Goals' }));
  expect(
    await section.findByText(
      'The selected profile requires additional context. Select its required input in document details, or choose a compatible profile.',
    ),
  ).toBeTruthy();
  await userEvent.click(section.getByText('Input details'));
  await userEvent.click(section.getByRole('combobox', { name: 'Profile input · Goals' }));
  await userEvent.click(screen.getByRole('option', { name: 'governing-guidance' }));
  expect(draft().root.nodes[0]!.context_binding!.entries['governing-guidance']).toBeTruthy();
  expect(
    section.queryByText(
      'The selected profile requires additional context. Select its required input in document details, or choose a compatible profile.',
    ),
  ).toBeNull();
  expect(draft().root.nodes[1]!.context_binding).toBeNull();
});

it('changes only the chosen legacy binding when the same document was explicitly attached twice', async () => {
  const { draft } = fixture((value) => {
    const source = {
      value: {
        mapping: 'constant' as const,
        value: {
          kind: 'artifact',
          value: {
            artifact_id: 'goals-v1',
            content_digest: 'a'.repeat(64),
            encoded_size: 20,
          },
        },
      },
    };
    value.root.nodes[0]!.context_binding = {
      entries: {
        'policy-guidance': { source_kind: 'policy', source },
        'reference-guidance': { source_kind: 'specification', source },
      },
    };
  });
  const documents = within(screen.getByRole('group', { name: 'Documents' }));
  await documents.findByRole('checkbox', { name: 'Goals' });
  const roles = documents.getAllByRole('combobox', { name: 'Document role · Goals' });
  await userEvent.click(roles[1]!);
  await userEvent.click(screen.getByRole('option', { name: 'Agent instructions' }));
  expect(draft().root.nodes[0]!.context_binding!.entries['reference-guidance']?.source_kind).toBe(
    'policy',
  );
  expect(draft().root.nodes[0]!.context_binding!.entries['policy-guidance']?.source_kind).toBe(
    'policy',
  );
  expect(Object.keys(draft().root.nodes[0]!.context_binding!.entries)).toHaveLength(2);
});

it('previews a saved exact selection without loading the catalog page that contains its metadata', async () => {
  const { documents, list, read, queryClient } = fixture((value) => {
    value.root.nodes[0]!.context_binding = {
      entries: {
        'retained-priorities': {
          source_kind: 'specification',
          source: {
            value: {
              mapping: 'constant',
              value: {
                kind: 'artifact',
                value: {
                  artifact_id: 'priorities-v1',
                  content_digest: 'a'.repeat(64),
                  encoded_size: 20,
                },
              },
            },
          },
        },
      },
    };
  });
  list.mockResolvedValue({ items: [documents[0]!], next_cursor: 'next-page' });
  await queryClient.invalidateQueries({ queryKey: queryKeys.projectInputDocuments('delivery') });
  const inputs = within(screen.getByRole('group', { name: 'Documents' }));
  const preview = inputs.getByRole('button', { name: 'Preview document' });
  expect(preview).toHaveProperty('disabled', false);
  expect(read).not.toHaveBeenCalled();
  await userEvent.click(preview);
  expect(await inputs.findByText('Exact priorities-v1 content')).toBeTruthy();
  expect(read).toHaveBeenCalledWith('delivery', 'priorities-v1');
  expect(list.mock.calls.some((call) => call[1] === 'next-page')).toBe(false);
  expect(inputs.queryByText('Retained document unavailable')).toBeNull();
});

it('shows a denied preview as a read-only preview failure without offering document editing in the step', async () => {
  const { read } = fixture();
  read.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      message: 'Read denied',
      requestId: 'preview-read',
      status: 403,
      retryAfterMilliseconds: null,
    }),
  );
  const documents = within(screen.getByRole('group', { name: 'Documents' }));
  await userEvent.click(await documents.findByRole('checkbox', { name: 'Goals' }));
  await userEvent.click(documents.getByRole('button', { name: 'Preview document' }));
  const failure = within(await screen.findByRole('alert'));
  expect(failure.getByText('Preview document')).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
});
