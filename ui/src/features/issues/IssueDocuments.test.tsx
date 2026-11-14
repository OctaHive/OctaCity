// @vitest-environment jsdom
import { act, cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi, test } from 'vitest';
import { queryKeys } from '../../app/query';
import type { Issue } from './api';
import type { IssueMaterialReference } from './documentsApi';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const original = {
  project_id: issue.project_id,
  issue_id: issue.id,
  name: 'Investigation',
  artifact: { artifact_id: 'issue-document-v1', content_digest: 'a'.repeat(64), encoded_size: 14 },
  actor: { kind: 'unauthenticated_management' as const, identity: null },
  published_at: 1,
  expires_at: 4102444800000,
};
function documents() {
  return {
    list: vi.fn().mockResolvedValue({ items: [original], next_cursor: null }),
    read: vi.fn().mockResolvedValue({ document: original, content: '# Private notes' }),
    publish: vi.fn(),
    editMaterials: vi.fn(),
  };
}
it('opens Issue-owned document history and reads one exact preview only on request', async () => {
  const docs = documents();
  const { api, queryClient } = setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    undefined,
    { issueDocumentsApi: docs },
  );
  const view = await screen.findByRole('region', { name: 'Issue documents' });
  expect(screen.getByRole('link', { name: 'Documents' }).getAttribute('aria-current')).toBe('page');
  expect(await within(view).findByText('Investigation')).toBeTruthy();
  expect(within(view).getByText('Issue-owned document')).toBeTruthy();
  expect(within(view).getByText('issue-document-v1')).toBeTruthy();
  expect(docs.read).not.toHaveBeenCalled();
  await userEvent.click(within(view).getByRole('button', { name: 'Preview Investigation' }));
  expect(await within(view).findByRole('heading', { name: 'Private notes' })).toBeTruthy();
  expect(docs.read).toHaveBeenCalledWith('delivery', issue.id, 'issue-document-v1');
  expect(
    JSON.stringify(
      queryClient
        .getQueryCache()
        .getAll()
        .map((q) => q.state.data),
    ),
  ).not.toContain('Private notes');
  expect(docs.publish).not.toHaveBeenCalled();
  expect(docs.editMaterials).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
});

it('creates an Issue publication with exact retry, preserves the draft and never selects or starts it', async () => {
  const docs = documents();
  docs.list.mockResolvedValue({ items: [], next_cursor: null });
  const { ManagementApiError } = await import('../../api/client');
  docs.publish
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Response lost',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockImplementationOnce(async () => {
      docs.list.mockResolvedValue({ items: [original], next_cursor: null });
      return original;
    });
  const { api, queryClient } = setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    undefined,
    { issueDocumentsApi: docs },
  );
  await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Investigation');
  await userEvent.type(screen.getByRole('textbox', { name: 'Document text' }), '# Private notes');
  await userEvent.click(screen.getByRole('button', { name: 'Preview' }));
  expect(screen.getByRole('region', { name: 'Document preview' }).textContent).toContain(
    'Private notes',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review Issue publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Write' }));
  expect(screen.getByRole('textbox', { name: 'Document text' })).toHaveProperty(
    'value',
    '# Private notes',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review Issue publication' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Publish Issue document' }));
  await userEvent.click(await dialog.findByRole('button', { name: 'Retry same command' }));
  expect(
    await dialog.findByText(
      'Issue document published. Material selection and execution are separate.',
    ),
  ).toBeTruthy();
  expect(docs.publish.mock.calls[0]).toEqual(docs.publish.mock.calls[1]);
  expect(docs.publish.mock.calls[0]?.slice(0, 3)).toEqual([
    'delivery',
    issue.id,
    { name: 'Investigation', content: '# Private notes' },
  ]);
  expect(docs.publish.mock.calls[0]?.[3]['If-Match']).toBeUndefined();
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  expect(await screen.findByText('issue-document-v1')).toBeTruthy();
  expect(docs.editMaterials).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
  expect(
    JSON.stringify(
      queryClient
        .getQueryCache()
        .getAll()
        .map((q) => q.state.data),
    ),
  ).not.toContain('Private notes');
  expect(JSON.stringify(localStorage) + JSON.stringify(sessionStorage)).not.toContain(
    'Private notes',
  );
});

it('selects, explicitly replaces and removes exact publications without changing historical inputs', async () => {
  const docs = documents();
  const newer = {
    ...original,
    artifact: { ...original.artifact, artifact_id: 'issue-document-v2' },
    published_at: 2,
  };
  docs.list.mockResolvedValue({ items: [original, newer], next_cursor: null });
  const { api } = setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  let current = { ...issue };
  api.get.mockImplementation(async () => current);
  docs.editMaterials.mockImplementation(async (_p, _i, body) => {
    current = {
      ...current,
      version: current.version + 1,
      materials: body.references.map((reference: unknown) => ({
        reference,
        selected_by: original.actor,
        selected_at: 10,
      })),
    };
    return { disposition: 'applied', resource: current };
  });
  const rows = await screen.findAllByRole('listitem');
  const oldRow = rows.find((el) => el.textContent?.includes('issue-document-v1'))!;
  const newRow = rows.find((el) => el.textContent?.includes('issue-document-v2'))!;
  await userEvent.click(
    within(oldRow).getByRole('button', { name: 'Select Investigation as material' }),
  );
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  await within(screen.getByRole('dialog')).findByText(
    'Material selection saved. No execution was started.',
  );
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  expect(docs.editMaterials.mock.calls[0]?.[3]['If-Match']).toBe('"1"');
  expect(docs.editMaterials.mock.calls[0]?.[2].references[0]).toEqual({
    kind: 'issue_document',
    project_id: issue.project_id,
    issue_id: issue.id,
    artifact: original.artifact,
  });
  expect(
    within(newRow).getByRole('button', { name: 'Select Investigation as material' }),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Replace issue-document-v1' }));
  await userEvent.click(within(newRow).getByRole('button', { name: 'Replace with Investigation' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  await within(screen.getByRole('dialog')).findByText(
    'Material selection saved. No execution was started.',
  );
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  expect(docs.editMaterials.mock.calls[1]?.[2].references).toEqual([
    {
      kind: 'issue_document',
      project_id: issue.project_id,
      issue_id: issue.id,
      artifact: newer.artifact,
    },
  ]);
  expect(docs.editMaterials.mock.calls[1]?.[3]['If-Match']).toBe('"2"');
  await userEvent.click(screen.getByRole('button', { name: 'Remove issue-document-v2' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  expect(
    await within(screen.getByRole('dialog')).findByText(
      'Material selection saved. No execution was started.',
    ),
  ).toBeTruthy();
  expect(docs.editMaterials.mock.calls[2]?.[2].references).toEqual([]);
  expect(docs.publish).not.toHaveBeenCalled();
});

it('links an explicitly previewed shared Project publication through its own owner without copying it', async () => {
  const { projectDocumentsApi } = await import('../../api/projectDocuments');
  const doc = {
    project_id: issue.project_id,
    name: 'Project guidance',
    artifact: { ...original.artifact, artifact_id: 'shared-document-v1' },
    published_at: 1,
    expires_at: original.expires_at,
  };
  const list = vi
    .spyOn(projectDocumentsApi, 'list')
    .mockResolvedValue({ items: [doc], next_cursor: null });
  const read = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: issue.project_id,
    artifact: doc.artifact,
    expires_at: original.expires_at,
    content: '# Shared private guidance',
  });
  const publish = vi.spyOn(projectDocumentsApi, 'publish');
  const docs = documents();
  docs.editMaterials.mockResolvedValue({
    disposition: 'applied',
    resource: { ...issue, version: 2 },
  });
  setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  await screen.findByRole('region', { name: 'Issue documents' });
  expect(list).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Link existing Project document' }));
  await userEvent.click(await screen.findByRole('button', { name: 'View Project guidance' }));
  expect(await screen.findByRole('heading', { name: 'Shared private guidance' })).toBeTruthy();
  expect(docs.editMaterials).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole('button', { name: 'Link this exact Project publication' }),
  );
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  expect(
    await within(screen.getByRole('dialog')).findByText(
      'Material selection saved. No execution was started.',
    ),
  ).toBeTruthy();
  expect(read).toHaveBeenCalledWith('delivery', 'shared-document-v1');
  expect(docs.editMaterials.mock.calls[0]?.[2].references).toEqual([
    { kind: 'project_document', project_id: issue.project_id, artifact: doc.artifact },
  ]);
  expect(docs.publish).not.toHaveBeenCalled();
  expect(publish).not.toHaveBeenCalled();
});

it('publishes a new Issue version from authorized preview without retargeting a selection', async () => {
  const docs = documents();
  const newer = {
    ...original,
    artifact: { ...original.artifact, artifact_id: 'issue-document-v2' },
  };
  docs.publish.mockImplementation(async () => {
    docs.list.mockResolvedValue({ items: [original, newer], next_cursor: null });
    return newer;
  });
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        materials: [
          {
            reference: {
              kind: 'issue_document',
              project_id: issue.project_id,
              issue_id: issue.id,
              artifact: original.artifact,
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      }),
    { issueDocumentsApi: docs },
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Preview Investigation' }));
  await screen.findByRole('heading', { name: 'Private notes' });
  await userEvent.click(screen.getByRole('button', { name: 'Publish new Issue version' }));
  const text = screen.getByRole('textbox', { name: 'Document text' });
  expect(text).toHaveProperty('value', '# Private notes');
  await userEvent.type(text, '\nNew version');
  await userEvent.click(screen.getByRole('button', { name: 'Review Issue publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish Issue document' }),
  );
  await within(screen.getByRole('dialog')).findByText(
    'Issue document published. Material selection and execution are separate.',
  );
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  expect(await screen.findByText('issue-document-v2')).toBeTruthy();
  expect(screen.getByRole('region', { name: 'Selected materials' }).textContent).toContain(
    'issue-document-v1',
  );
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

it('preserves an unpublished draft while switching Issue sections and clears it when leaving the Issue', async () => {
  const docs = documents();
  const { router } = setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Unsaved');
  await userEvent.type(screen.getByRole('textbox', { name: 'Document text' }), 'private draft');
  await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
  await userEvent.click(screen.getByRole('link', { name: 'Documents' }));
  expect(await screen.findByRole('textbox', { name: 'Document text' })).toHaveProperty(
    'value',
    'private draft',
  );
  await router.navigate('/projects');
  await screen.findByRole('heading', { name: 'Projects', level: 1 });
  await router.navigate(`/projects/delivery/issues/${issue.id}?section=documents`);
  expect(await screen.findByRole('button', { name: 'New Issue document' })).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
  expect(docs.publish).not.toHaveBeenCalled();
});

it('shows exact historical selection and unavailable content without offering current writes', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const docs = documents();
  docs.list.mockResolvedValue({ items: [], next_cursor: null });
  docs.read.mockRejectedValue(
    new ManagementApiError({
      code: 'not_found',
      message: 'Publication expired',
      status: 404,
      requestId: 'expired',
      retryAfterMilliseconds: null,
    }),
  );
  const { api } = setup(
    `/projects/delivery/issues/${issue.id}?section=documents&revision=2`,
    (api) => {
      api.get.mockResolvedValue({ ...issue, version: 4 });
      api.revision.mockResolvedValue({
        ...issue,
        version: 2,
        materials: [
          {
            reference: {
              kind: 'issue_document',
              project_id: issue.project_id,
              issue_id: issue.id,
              artifact: original.artifact,
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      });
    },
    { issueDocumentsApi: docs },
  );
  expect(await screen.findByText('Historical document selection · revision 2')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'New Issue document' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Link existing Project document' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Remove issue-document-v1' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Preview issue-document-v1' }));
  expect(await screen.findByText('Publication expired')).toBeTruthy();
  expect(docs.read).toHaveBeenCalledWith('delivery', issue.id, 'issue-document-v1');
  expect(api.revision).toHaveBeenCalledWith('delivery', issue.id, 2, expect.any(AbortSignal));
  expect(docs.publish).not.toHaveBeenCalled();
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

it('uses generated UTF-8 publication ceilings before review', async () => {
  const { fireEvent } = await import('@testing-library/react');
  const docs = documents();
  setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'Document name' }), {
    target: { value: 'é'.repeat(129) },
  });
  fireEvent.change(screen.getByRole('textbox', { name: 'Document text' }), {
    target: { value: 'private draft' },
  });
  expect(screen.getByRole('button', { name: 'Review Issue publication' })).toHaveProperty(
    'disabled',
    true,
  );
  fireEvent.change(screen.getByRole('textbox', { name: 'Document name' }), {
    target: { value: 'Bounded' },
  });
  fireEvent.change(screen.getByRole('textbox', { name: 'Document text' }), {
    target: { value: 'é'.repeat(524289) },
  });
  expect(screen.getByRole('button', { name: 'Review Issue publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(docs.publish).not.toHaveBeenCalled();
});

it('keeps an exact failed selection for retry and preserves a publication draft across a material conflict', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const docs = documents();
  docs.editMaterials.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Issue changed',
      status: 409,
      requestId: 'conflict',
      retryAfterMilliseconds: null,
    }),
  );
  const { api } = setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Unsaved');
  await userEvent.type(screen.getByRole('textbox', { name: 'Document text' }), 'private draft');
  await userEvent.click(
    await screen.findByRole('button', { name: 'Select Investigation as material' }),
  );
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  expect(await screen.findByText('Issue changed')).toBeTruthy();
  api.get.mockResolvedValue({ ...issue, version: 2 });
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Refresh current data' }),
  );
  expect(await screen.findByRole('textbox', { name: 'Document text' })).toHaveProperty(
    'value',
    'private draft',
  );
  docs.editMaterials
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Ack lost',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValueOnce({ disposition: 'replayed', resource: { ...issue, version: 3 } });
  await userEvent.click(screen.getByRole('button', { name: 'Select Investigation as material' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Save material selection' }));
  await userEvent.click(await dialog.findByRole('button', { name: 'Retry same command' }));
  expect(
    await dialog.findByText('Material selection saved. No execution was started.'),
  ).toBeTruthy();
  expect(docs.editMaterials.mock.calls[1]).toEqual(docs.editMaterials.mock.calls[2]);
  expect(docs.editMaterials.mock.calls[1]?.[3]['If-Match']).toBe('"2"');
  expect(docs.publish).not.toHaveBeenCalled();
});

it('previews an exact shared material absent from the Project catalog in historical selection', async () => {
  const { projectDocumentsApi } = await import('../../api/projectDocuments');
  const list = vi.spyOn(projectDocumentsApi, 'list');
  const read = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: issue.project_id,
    artifact: original.artifact,
    expires_at: original.expires_at,
    content: '# Old shared publication',
  });
  const docs = documents();
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents&revision=2`,
    (api) => {
      api.get.mockResolvedValue({ ...issue, version: 4 });
      api.revision.mockResolvedValue({
        ...issue,
        version: 2,
        materials: [
          {
            reference: {
              kind: 'project_document',
              project_id: issue.project_id,
              artifact: original.artifact,
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      });
    },
    { issueDocumentsApi: docs },
  );
  const selected = await screen.findByRole('region', { name: 'Selected materials' });
  await userEvent.click(
    await within(selected).findByRole('button', { name: 'Preview issue-document-v1' }),
  );
  expect(await screen.findByRole('heading', { name: 'Old shared publication' })).toBeTruthy();
  expect(read).toHaveBeenCalledWith('delivery', 'issue-document-v1');
  expect(list).not.toHaveBeenCalled();
  expect(docs.read).not.toHaveBeenCalled();
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

it('reviews exact source owners and digests and labels selection attribution', async () => {
  const docs = documents();
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        materials: [
          {
            reference: {
              kind: 'comment',
              project_id: issue.project_id,
              issue_id: issue.id,
              comment_id: 'note',
              body_digest: 'b'.repeat(64),
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      }),
    { issueDocumentsApi: docs },
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Select Investigation as material' }),
  );
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText(original.artifact.content_digest)).toBeTruthy();
  expect(dialog.getAllByText(issue.project_id)).toBeTruthy();
  expect(dialog.getAllByText(issue.id)).toBeTruthy();
  expect(
    within(screen.getByRole('region', { name: 'Selected materials' })).getByText(
      /Unauthenticated management/,
    ),
  ).toBeTruthy();
});

it('removes only the reviewed source kind when comment and document identifiers coincide', async () => {
  const docs = documents();
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        materials: [
          {
            reference: {
              kind: 'comment',
              project_id: issue.project_id,
              issue_id: issue.id,
              comment_id: original.artifact.artifact_id,
              body_digest: 'b'.repeat(64),
            },
            selected_by: original.actor,
            selected_at: 1,
          },
          {
            reference: {
              kind: 'issue_document',
              project_id: issue.project_id,
              issue_id: issue.id,
              artifact: original.artifact,
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      }),
    { issueDocumentsApi: docs },
  );
  const selected = await screen.findByRole('region', { name: 'Selected materials' });
  const comment = within(selected)
    .getAllByRole('listitem')
    .find((el) => el.textContent?.includes('Issue comment'))!;
  await userEvent.click(within(comment).getByRole('button', { name: 'Remove issue-document-v1' }));
  docs.editMaterials.mockResolvedValue({
    disposition: 'applied',
    resource: { ...issue, version: 2 },
  });
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
  );
  await within(screen.getByRole('dialog')).findByText(
    'Material selection saved. No execution was started.',
  );
  expect(docs.editMaterials.mock.calls[0]?.[2].references).toEqual([
    {
      kind: 'issue_document',
      project_id: issue.project_id,
      issue_id: issue.id,
      artifact: original.artifact,
    },
  ]);
});

test.each(['publication', 'selection'])(
  'keeps the frozen %s review visible across Issue section navigation',
  async (kind) => {
    const docs = documents();
    const { router } = setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
      issueDocumentsApi: docs,
    });
    if (kind === 'publication') {
      await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
      await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Unsaved');
      await userEvent.type(screen.getByRole('textbox', { name: 'Document text' }), 'private draft');
      await userEvent.click(screen.getByRole('button', { name: 'Review Issue publication' }));
    } else
      await userEvent.click(
        await screen.findByRole('button', { name: 'Select Investigation as material' }),
      );
    await router.navigate(`/projects/delivery/issues/${issue.id}?section=overview`);
    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Overview' }).getAttribute('aria-current')).toBe(
        'page',
      ),
    );
    expect(await screen.findByRole('dialog')).toBeTruthy();
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }),
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    await router.navigate(`/projects/delivery/issues/${issue.id}?section=documents`);
    if (kind === 'publication')
      expect(await screen.findByRole('textbox', { name: 'Document text' })).toHaveProperty(
        'value',
        'private draft',
      );
    expect(docs.publish).not.toHaveBeenCalled();
    expect(docs.editMaterials).not.toHaveBeenCalled();
  },
);

test.each(['project', 'issue', 'id', 'digest', 'size'])(
  'rejects an Issue preview with mismatched %s metadata without displaying its body',
  async (field) => {
    const docs = documents();
    const returned = { ...original, artifact: { ...original.artifact } };
    if (field === 'project') returned.project_id = 'foreign';
    if (field === 'issue') returned.issue_id = 'foreign';
    if (field === 'id') returned.artifact.artifact_id = 'newer';
    if (field === 'digest') returned.artifact.content_digest = 'f'.repeat(64);
    if (field === 'size') returned.artifact.encoded_size++;
    docs.read.mockResolvedValue({ document: returned, content: '# Foreign private text' });
    const { queryClient } = setup(
      `/projects/delivery/issues/${issue.id}?section=documents`,
      undefined,
      { issueDocumentsApi: docs },
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Preview Investigation' }));
    expect((await screen.findByRole('alert')).getAttribute('data-state')).toBe('internal-error');
    expect(screen.queryByRole('heading', { name: 'Foreign private text' })).toBeNull();
    expect(
      JSON.stringify(
        queryClient
          .getQueryCache()
          .getAll()
          .map((q) => q.state.data),
      ),
    ).not.toContain('Foreign private text');
  },
);

it('can link a fresh shared publication after an earlier preview expires', async () => {
  const { projectDocumentsApi } = await import('../../api/projectDocuments');
  const old = {
    project_id: issue.project_id,
    name: 'Old shared',
    artifact: { ...original.artifact, artifact_id: 'old-shared' },
    published_at: 1,
    expires_at: Date.now() + 1500,
  };
  const fresh = {
    ...old,
    name: 'Fresh shared',
    artifact: { ...original.artifact, artifact_id: 'fresh-shared' },
    expires_at: original.expires_at,
  };
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [old, fresh],
    next_cursor: null,
  });
  vi.spyOn(projectDocumentsApi, 'read').mockImplementation(async (_project, id) => ({
    project_id: issue.project_id,
    artifact: id === 'old-shared' ? old.artifact : fresh.artifact,
    expires_at: id === 'old-shared' ? old.expires_at : fresh.expires_at,
    content: '# Shared content',
  }));
  setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: documents(),
  });
  await userEvent.click(
    await screen.findByRole('button', { name: 'Link existing Project document' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'View Old shared' }));
  await screen.findByRole('heading', { name: 'Shared content' });
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Link this exact Project publication' }),
    ).toHaveProperty('disabled', true),
  );
  await userEvent.click(screen.getByRole('button', { name: 'View Fresh shared' }));
  await screen.findByRole('article', { name: 'Fresh shared' });
  expect(
    screen.getByRole('button', { name: 'Link this exact Project publication' }),
  ).toHaveProperty('disabled', false);
});

it('does not offer a duplicate shared publication as another material', async () => {
  const { projectDocumentsApi } = await import('../../api/projectDocuments');
  const shared = {
    project_id: issue.project_id,
    name: 'Shared',
    artifact: original.artifact,
    published_at: 1,
    expires_at: original.expires_at,
  };
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [shared], next_cursor: null });
  vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: issue.project_id,
    artifact: shared.artifact,
    expires_at: original.expires_at,
    content: '# Shared content',
  });
  const docs = documents();
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        materials: [
          {
            reference: {
              kind: 'project_document',
              project_id: issue.project_id,
              artifact: shared.artifact,
            },
            selected_by: original.actor,
            selected_at: 1,
          },
        ],
      }),
    { issueDocumentsApi: docs },
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Link existing Project document' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'View Shared' }));
  await screen.findByRole('heading', { name: 'Shared content' });
  expect(
    screen.getByRole('button', { name: 'Link this exact Project publication' }),
  ).toHaveProperty('disabled', true);
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

it('keeps a full selection editable by replacement while preventing an extra material', async () => {
  const docs = documents();
  setup(
    `/projects/delivery/issues/${issue.id}?section=documents`,
    (api) =>
      api.get.mockResolvedValue({
        ...issue,
        materials: Array.from({ length: 32 }, (_, index) => ({
          reference: {
            kind: 'issue_document',
            project_id: issue.project_id,
            issue_id: issue.id,
            artifact: { ...original.artifact, artifact_id: `selected-${index}` },
          },
          selected_by: original.actor,
          selected_at: 1,
        })),
      }),
    { issueDocumentsApi: docs },
  );
  expect(
    await screen.findByRole('button', { name: 'Select Investigation as material' }),
  ).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Replace selected-0' }));
  await userEvent.click(screen.getByRole('button', { name: 'Replace with Investigation' }));
  expect(within(screen.getByRole('dialog')).getAllByRole('listitem')).toHaveLength(32);
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

it('shows denied preview as unavailable without affecting the retained draft or selecting content', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const docs = documents();
  docs.read.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      message: 'Access denied',
      status: 403,
      requestId: 'denied',
      retryAfterMilliseconds: null,
    }),
  );
  setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
    issueDocumentsApi: docs,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document text' }), 'private draft');
  await userEvent.click(screen.getByRole('button', { name: 'Preview Investigation' }));
  expect((await screen.findByRole('alert')).getAttribute('data-state')).toBe('forbidden');
  expect(screen.getByRole('textbox', { name: 'Document text' })).toHaveProperty(
    'value',
    'private draft',
  );
  expect(docs.editMaterials).not.toHaveBeenCalled();
});

test.each(['forbidden', 'not_found'] as const)(
  'clears a shared preview and its linking action when an explicit reread is %s',
  async (code) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const { ManagementApiError } = await import('../../api/client');
    const shared = {
      project_id: issue.project_id,
      name: 'Shared source',
      artifact: original.artifact,
      published_at: 1,
      expires_at: original.expires_at,
    };
    vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [shared], next_cursor: null });
    const read = vi
      .spyOn(projectDocumentsApi, 'read')
      .mockResolvedValueOnce({
        project_id: issue.project_id,
        artifact: shared.artifact,
        expires_at: original.expires_at,
        content: '# Previously authorized text',
      })
      .mockRejectedValueOnce(
        new ManagementApiError({
          code,
          status: code === 'forbidden' ? 403 : 404,
          message: 'Source no longer available',
          requestId: 'unavailable',
          retryAfterMilliseconds: null,
        }),
      );
    const docs = documents();
    setup(`/projects/delivery/issues/${issue.id}?section=documents`, undefined, {
      issueDocumentsApi: docs,
    });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Link existing Project document' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'View Shared source' }));
    await screen.findByRole('heading', { name: 'Previously authorized text' });
    await userEvent.click(screen.getByRole('button', { name: 'View Shared source' }));
    await screen.findByText('Source no longer available');
    expect(screen.queryByRole('heading', { name: 'Previously authorized text' })).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Link this exact Project publication' }),
    ).toBeNull();
    expect(read).toHaveBeenCalledTimes(2);
    expect(docs.editMaterials).not.toHaveBeenCalled();
    // A fresh successful explicit read may offer the exact source again.
    read.mockResolvedValueOnce({
      project_id: issue.project_id,
      artifact: shared.artifact,
      expires_at: original.expires_at,
      content: '# Available again',
    });
    await userEvent.click(screen.getByRole('button', { name: 'View Shared source' }));
    await screen.findByRole('heading', { name: 'Available again' });
    expect(
      screen.getByRole('button', { name: 'Link this exact Project publication' }),
    ).toHaveProperty('disabled', false);
  },
);

test.each(['editing', 'review', 'retry'] as const)(
  'retains the %s publication state across a same-Issue historical round trip',
  async (phase) => {
    const { ManagementApiError } = await import('../../api/client');
    const docs = documents();
    docs.publish
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'transport_failure',
          message: 'Acknowledgement lost',
          status: null,
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValueOnce(original);
    const path = `/projects/delivery/issues/${issue.id}?section=documents`;
    const { router } = setup(
      path,
      (api) => {
        api.get.mockResolvedValue({ ...issue, version: 3 });
        api.revision.mockResolvedValue({ ...issue, version: 1 });
      },
      { issueDocumentsApi: docs },
    );
    await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Unpublished');
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Document text' }),
      'PRIVATE_UNSAVED_HISTORY_DRAFT',
    );
    if (phase !== 'editing')
      await userEvent.click(screen.getByRole('button', { name: 'Review Issue publication' }));
    if (phase === 'retry') {
      await userEvent.click(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish Issue document' }),
      );
      await within(screen.getByRole('dialog')).findByRole('button', { name: 'Retry same command' });
    }
    await router.navigate(path + '&revision=1');
    await screen.findByText('Historical document selection · revision 1');
    expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
    if (phase === 'review')
      expect(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish Issue document' }),
      ).toHaveProperty('disabled', true);
    if (phase === 'retry')
      expect(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Retry same command' }),
      ).toHaveProperty('disabled', false);
    await router.navigate(path);
    expect(await screen.findByRole('textbox', { name: 'Document text' })).toHaveProperty(
      'value',
      'PRIVATE_UNSAVED_HISTORY_DRAFT',
    );
    expect(screen.getByRole('textbox', { name: 'Document name' })).toHaveProperty(
      'value',
      'Unpublished',
    );
    if (phase === 'review') {
      expect(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish Issue document' }),
      ).toHaveProperty('disabled', false);
      await userEvent.click(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }),
      );
    }
    if (phase === 'retry') {
      await userEvent.click(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Retry same command' }),
      );
      await within(screen.getByRole('dialog')).findByText(
        'Issue document published. Material selection and execution are separate.',
      );
      expect(docs.publish.mock.calls[1]).toEqual(docs.publish.mock.calls[0]);
      expect(docs.publish.mock.calls[1]?.[2]).toEqual({
        name: 'Unpublished',
        content: 'PRIVATE_UNSAVED_HISTORY_DRAFT',
      });
      expect(docs.publish.mock.calls[1]?.[3]['If-Match']).toBeUndefined();
    } else expect(docs.publish).not.toHaveBeenCalled();
    expect(docs.editMaterials).not.toHaveBeenCalled();
  },
);

test.each(['issue_document', 'project_document'] as const)(
  'requires a new explicit choice when a %s replacement target disappears while Documents is hidden',
  async (kind) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const shared = {
      project_id: issue.project_id,
      name: 'Shared replacement',
      artifact: { ...original.artifact, artifact_id: 'shared-replacement' },
      published_at: 1,
      expires_at: original.expires_at,
    };
    vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [shared], next_cursor: null });
    vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
      project_id: issue.project_id,
      artifact: shared.artifact,
      expires_at: original.expires_at,
      content: '# Replacement guidance',
    });
    const removed: IssueMaterialReference =
      kind === 'issue_document'
        ? {
            kind,
            project_id: issue.project_id,
            issue_id: issue.id,
            artifact: { ...original.artifact, artifact_id: 'removed-target' },
          }
        : {
            kind,
            project_id: issue.project_id,
            artifact: { ...original.artifact, artifact_id: 'removed-target' },
          };
    const retained: IssueMaterialReference = {
      kind: 'comment',
      project_id: issue.project_id,
      issue_id: issue.id,
      comment_id: 'retained-comment',
      body_digest: 'b'.repeat(64),
    };
    let current: Issue = {
      ...issue,
      materials: [removed, retained].map((reference) => ({
        reference,
        selected_by: original.actor,
        selected_at: 1,
      })),
    };
    const docs = documents();
    docs.editMaterials.mockImplementation(async (_project, _issue, body) => {
      current = {
        ...current,
        version: 3,
        materials: body.references.map((reference: IssueMaterialReference) => ({
          reference,
          selected_by: original.actor,
          selected_at: 3,
        })),
      };
      return { disposition: 'applied', resource: current };
    });
    setup(
      `/projects/delivery/issues/${issue.id}?section=documents`,
      (api) => api.get.mockImplementation(async () => current),
      { issueDocumentsApi: docs },
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Link existing Project document' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'View Shared replacement' }));
    await screen.findByRole('heading', { name: 'Replacement guidance' });
    await userEvent.click(screen.getByRole('button', { name: 'Replace removed-target' }));
    await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
    await screen.findByText('Version 1');
    current = {
      ...current,
      version: 2,
      materials: [{ reference: retained, selected_by: original.actor, selected_at: 1 }],
    };
    await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
    await screen.findByText('Version 2');
    await userEvent.click(screen.getByRole('link', { name: 'Documents' }));
    await screen.findByText(
      'The material to replace is no longer selected. Cancel replacement and choose again.',
    );
    const replace = screen.getByRole('button', { name: 'Replace with Investigation' });
    expect(replace).toHaveProperty('disabled', true);
    expect(
      screen.getByRole('button', { name: 'Link this exact Project publication' }),
    ).toHaveProperty('disabled', true);
    await userEvent.click(replace);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(docs.editMaterials).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await userEvent.click(screen.getByRole('button', { name: 'Select Investigation as material' }));
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
    );
    await within(screen.getByRole('dialog')).findByText(
      'Material selection saved. No execution was started.',
    );
    expect(docs.editMaterials.mock.calls[0]?.[2].references).toEqual([
      retained,
      {
        kind: 'issue_document',
        project_id: issue.project_id,
        issue_id: issue.id,
        artifact: original.artifact,
      },
    ]);
    expect(docs.editMaterials.mock.calls[0]?.[3]['If-Match']).toBe('"2"');
    expect(docs.publish).not.toHaveBeenCalled();
  },
);

test.each(['review', 'retry'] as const)(
  'retains the frozen material %s across a historical round trip without a new command identity',
  async (phase) => {
    const { ManagementApiError } = await import('../../api/client');
    const docs = documents();
    docs.editMaterials
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'transport_failure',
          message: 'Selection acknowledgement lost',
          status: null,
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValueOnce({ disposition: 'replayed', resource: { ...issue, version: 4 } });
    const path = `/projects/delivery/issues/${issue.id}?section=documents`;
    const { router } = setup(
      path,
      (api) => {
        api.get.mockResolvedValue({ ...issue, version: 3 });
        api.revision.mockResolvedValue({ ...issue, version: 1 });
      },
      { issueDocumentsApi: docs },
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Select Investigation as material' }),
    );
    if (phase === 'retry') {
      await userEvent.click(
        within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
      );
      await within(screen.getByRole('dialog')).findByRole('button', { name: 'Retry same command' });
    }
    await router.navigate(path + '&revision=1');
    await screen.findByText('Historical document selection · revision 1');
    const action = phase === 'review' ? 'Save material selection' : 'Retry same command';
    expect(within(screen.getByRole('dialog')).getByRole('button', { name: action })).toHaveProperty(
      'disabled',
      phase === 'review',
    );
    await router.navigate(path);
    await waitFor(() =>
      expect(
        within(screen.getByRole('dialog')).getByRole('button', { name: action }),
      ).toHaveProperty('disabled', false),
    );
    if (phase === 'retry') {
      await userEvent.click(
        within(screen.getByRole('dialog')).getByRole('button', { name: action }),
      );
      await within(screen.getByRole('dialog')).findByText(
        'Material selection saved. No execution was started.',
      );
      expect(docs.editMaterials.mock.calls[1]).toEqual(docs.editMaterials.mock.calls[0]);
      expect(docs.editMaterials.mock.calls[1]?.[3]['If-Match']).toBe('"3"');
    } else expect(docs.editMaterials).not.toHaveBeenCalled();
  },
);

test.each(['success', 'failure'] as const)(
  'releases preview controls when a selected Project reference is removed before its read finishes with %s',
  async (outcome) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const { ManagementApiError } = await import('../../api/client');
    type Body = Awaited<ReturnType<typeof projectDocumentsApi.read>>;
    let resolveRead!: (body: Body) => void;
    let rejectRead!: (error: Error) => void;
    const pending = new Promise<Body>((resolve, reject) => {
      resolveRead = resolve;
      rejectRead = reject;
    });
    vi.spyOn(projectDocumentsApi, 'read').mockReturnValue(pending);
    const shared: IssueMaterialReference = {
      kind: 'project_document',
      project_id: issue.project_id,
      artifact: { ...original.artifact, artifact_id: 'selected-shared' },
    };
    let current: Issue = {
      ...issue,
      materials: [{ reference: shared, selected_by: original.actor, selected_at: 1 }],
    };
    const docs = documents();
    docs.editMaterials.mockImplementation(async () => {
      current = { ...current, version: 2, materials: [] };
      return { disposition: 'applied', resource: current };
    });
    setup(
      `/projects/delivery/issues/${issue.id}?section=documents`,
      (api) => api.get.mockImplementation(async () => current),
      { issueDocumentsApi: docs },
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Preview selected-shared' }));
    expect(screen.getByRole('button', { name: 'Preview Investigation' })).toHaveProperty(
      'disabled',
      true,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Remove selected-shared' }));
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Save material selection' }),
    );
    await within(screen.getByRole('dialog')).findByText(
      'Material selection saved. No execution was started.',
    );
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }),
    );
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Preview selected-shared' })).toBeNull(),
    );
    await act(async () => {
      if (outcome === 'success')
        resolveRead({
          project_id: issue.project_id,
          artifact: shared.artifact,
          expires_at: original.expires_at,
          content: '# Explicit shared preview',
        });
      else
        rejectRead(
          new ManagementApiError({
            code: 'not_found',
            status: 404,
            message: 'Shared source expired',
            requestId: 'expired',
            retryAfterMilliseconds: null,
          }),
        );
      await pending.catch(() => {});
    });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Preview Investigation' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    // A different independently requested preview must still work after row removal.
    await userEvent.click(screen.getByRole('button', { name: 'Preview Investigation' }));
    expect(await screen.findByRole('heading', { name: 'Private notes' })).toBeTruthy();
    expect(docs.editMaterials).toHaveBeenCalledTimes(1);
    expect(docs.publish).not.toHaveBeenCalled();
  },
);

test.each(['issue_document', 'project_document'] as const)(
  'renders historical %s preview independently of the preserved current editor',
  async (kind) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const source: IssueMaterialReference =
      kind === 'issue_document'
        ? { kind, project_id: issue.project_id, issue_id: issue.id, artifact: original.artifact }
        : {
            kind,
            project_id: issue.project_id,
            artifact: { ...original.artifact, artifact_id: 'historical-project-source' },
          };
    const projectRead = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
      project_id: issue.project_id,
      artifact: source.artifact,
      expires_at: original.expires_at,
      content: '# Historical Project context',
    });
    const docs = documents();
    const path = `/projects/delivery/issues/${issue.id}?section=documents`;
    const { router } = setup(
      path,
      (api) => {
        api.get.mockResolvedValue({ ...issue, version: 2 });
        api.revision.mockResolvedValue({
          ...issue,
          version: 1,
          materials: [{ reference: source, selected_by: original.actor, selected_at: 1 }],
        });
      },
      { issueDocumentsApi: docs },
    );
    await userEvent.click(await screen.findByRole('button', { name: 'New Issue document' }));
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Document name' }),
      'Unpublished current document',
    );
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Document text' }),
      'PRESERVED_CURRENT_DRAFT',
    );
    await router.navigate(path + '&revision=1');
    await screen.findByText('Historical document selection · revision 1');
    expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
    await userEvent.click(
      within(screen.getByRole('region', { name: 'Selected materials' })).getByRole('button', {
        name: `Preview ${source.artifact.artifact_id}`,
      }),
    );
    const title = kind === 'issue_document' ? 'Private notes' : 'Historical Project context';
    expect(await screen.findByRole('heading', { name: title })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Publish new Issue version' })).toBeNull();
    if (kind === 'issue_document') expect(projectRead).not.toHaveBeenCalled();
    else expect(docs.read).not.toHaveBeenCalled();
    await router.navigate(path);
    expect(await screen.findByRole('textbox', { name: 'Document name' })).toHaveProperty(
      'value',
      'Unpublished current document',
    );
    expect(screen.getByRole('textbox', { name: 'Document text' })).toHaveProperty(
      'value',
      'PRESERVED_CURRENT_DRAFT',
    );
    expect(screen.queryByRole('heading', { name: title })).toBeNull();
    expect(docs.publish).not.toHaveBeenCalled();
    expect(docs.editMaterials).not.toHaveBeenCalled();
  },
);

test.each(['success', 'failure', 'removed'] as const)(
  'serializes shared publication reads and releases the picker after the active read ends with %s',
  async (outcome) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const { ManagementApiError } = await import('../../api/client');
    const first = {
      project_id: issue.project_id,
      name: 'Slow shared A',
      artifact: { ...original.artifact, artifact_id: 'shared-a' },
      published_at: 1,
      expires_at: original.expires_at,
    };
    const second = {
      ...first,
      name: 'Shared B',
      artifact: { ...first.artifact, artifact_id: 'shared-b' },
    };
    const list = vi
      .spyOn(projectDocumentsApi, 'list')
      .mockResolvedValue({ items: [first, second], next_cursor: null });
    type Body = Awaited<ReturnType<typeof projectDocumentsApi.read>>;
    let resolveFirst!: (value: Body) => void;
    let rejectFirst!: (error: Error) => void;
    let resolveSecond!: (value: Body) => void;
    const pendingFirst = new Promise<Body>((resolve, reject) => {
      resolveFirst = resolve;
      rejectFirst = reject;
    });
    const pendingSecond = new Promise<Body>((resolve) => {
      resolveSecond = resolve;
    });
    const read = vi
      .spyOn(projectDocumentsApi, 'read')
      .mockImplementation(async (_project, id) =>
        id === 'shared-a' ? pendingFirst : pendingSecond,
      );
    const docs = documents();
    const { queryClient } = setup(
      `/projects/delivery/issues/${issue.id}?section=documents`,
      undefined,
      { issueDocumentsApi: docs },
    );
    const toggle = await screen.findByRole('button', { name: 'Link existing Project document' });
    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'View Slow shared A' }));
    const chooseSecond = screen.getByRole('button', { name: 'View Shared B' });
    expect(chooseSecond).toHaveProperty('disabled', true);
    expect(toggle).toHaveProperty('disabled', true);
    await userEvent.click(chooseSecond);
    expect(read.mock.calls.map((call) => call[1])).toEqual(['shared-a']);
    if (outcome === 'removed') {
      list.mockResolvedValue({ items: [second], next_cursor: null });
      await queryClient.invalidateQueries({
        queryKey: queryKeys.projectInputDocuments(issue.project_id),
      });
      await waitFor(() =>
        expect(screen.queryByRole('button', { name: 'View Slow shared A' })).toBeNull(),
      );
    }
    await act(async () => {
      if (outcome !== 'failure')
        resolveFirst({
          project_id: issue.project_id,
          artifact: first.artifact,
          expires_at: first.expires_at,
          content: '# First explicit publication',
        });
      else
        rejectFirst(
          new ManagementApiError({
            code: 'forbidden',
            status: 403,
            message: 'First publication unavailable',
            requestId: 'denied',
            retryAfterMilliseconds: null,
          }),
        );
      await pendingFirst.catch(() => {});
    });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'View Shared B' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    expect(toggle).toHaveProperty('disabled', false);
    await userEvent.click(screen.getByRole('button', { name: 'View Shared B' }));
    expect(
      screen.queryByRole('button', { name: 'Link this exact Project publication' }),
    ).toBeNull();
    await act(async () => {
      resolveSecond({
        project_id: issue.project_id,
        artifact: second.artifact,
        expires_at: second.expires_at,
        content: '# Last explicit publication',
      });
      await pendingSecond;
    });
    await screen.findByRole('heading', { name: 'Last explicit publication' });
    expect(screen.queryByRole('heading', { name: 'First explicit publication' })).toBeNull();
    expect(read.mock.calls.map((call) => call[1])).toEqual(['shared-a', 'shared-b']);
    await userEvent.click(
      screen.getByRole('button', { name: 'Link this exact Project publication' }),
    );
    const dialog = within(screen.getByRole('dialog'));
    expect(dialog.getByText('shared-b')).toBeTruthy();
    expect(dialog.queryByText('shared-a')).toBeNull();
    expect(docs.editMaterials).not.toHaveBeenCalled();
  },
);

test.each(['current', 'historical'] as const)(
  'hides a selected Project preview at its owner deadline in the %s revision without catalog lookup',
  async (revision) => {
    const { projectDocumentsApi } = await import('../../api/projectDocuments');
    const now = 1800000000000;
    const clock = vi.spyOn(Date, 'now').mockReturnValue(now);
    const artifact = { ...original.artifact, artifact_id: 'retained-project-selection' };
    const body = {
      project_id: issue.project_id,
      artifact,
      expires_at: now + 1000,
      content: '# Retained shared context',
    };
    const read = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue(body);
    const list = vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
      items: [],
      next_cursor: null,
    });
    const selected: Issue = {
      ...issue,
      version: 1,
      materials: [
        {
          reference: { kind: 'project_document', project_id: issue.project_id, artifact },
          selected_by: original.actor,
          selected_at: 1,
        },
      ],
    };
    const docs = documents();
    const { api } = setup(
      `/projects/delivery/issues/${issue.id}?section=documents${revision === 'historical' ? '&revision=1' : ''}`,
      (api) => {
        api.get.mockResolvedValue(revision === 'historical' ? { ...issue, version: 2 } : selected);
        api.revision.mockResolvedValue(selected);
      },
      { issueDocumentsApi: docs },
    );
    await userEvent.click(
      await screen.findByRole('button', {
        name: 'Preview retained-project-selection',
      }),
    );
    expect(await screen.findByRole('heading', { name: 'Retained shared context' })).toBeTruthy();
    clock.mockReturnValue(now + 1000);
    await waitFor(
      () => {
        expect(screen.queryByRole('heading', { name: 'Retained shared context' })).toBeNull();
      },
      { timeout: 2500 },
    );
    expect(
      within(screen.getByRole('article', { name: artifact.artifact_id })).getByRole('status')
        .textContent,
    ).toBe('This retained document has expired. Publish or select an available version.');
    expect(read).toHaveBeenCalledWith(issue.project_id, artifact.artifact_id);
    expect(list).not.toHaveBeenCalled();
    expect(docs.publish).not.toHaveBeenCalled();
    expect(docs.editMaterials).not.toHaveBeenCalled();
    expect(api.edit).not.toHaveBeenCalled();
  },
);
