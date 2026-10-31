// @vitest-environment jsdom

import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';

import { projectDocumentsApi } from '../../api/projectDocuments';
import { ManagementApiError } from '../../api/client';
import { fakeProjectsApi, project, renderProjects } from './ProjectTestSupport';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function retainedDocument() {
  return {
    project_id: 'delivery',
    name: 'Components',
    artifact: { artifact_id: 'components-v1', content_digest: 'd'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  };
}

async function openRetainedDocument() {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [retainedDocument()],
    next_cursor: null,
  });
  const view = renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Retained Project document' }));
  await userEvent.click(screen.getByRole('option', { name: /Components/ }));
  await userEvent.click(
    screen.getByRole('button', { name: 'Open document for viewing or editing' }),
  );
  return view;
}

it('views exact content and publishes an edit as a new document without repinning the selected version or persisting private text', async () => {
  const original = retainedDocument();
  const read = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: original.project_id,
    artifact: original.artifact,
    content: 'Console and storage\n',
  });
  const publish = vi.spyOn(projectDocumentsApi, 'publish').mockResolvedValue({
    ...original,
    artifact: { ...original.artifact, artifact_id: 'components-v2' },
  });
  const storage = vi.spyOn(Storage.prototype, 'setItem');
  const { queryClient } = await openRetainedDocument();
  expect(read).toHaveBeenCalledWith('delivery', 'components-v1');
  const content = screen.getByRole('textbox', { name: 'Document text' });
  await waitFor(() => expect(content.textContent).toBe('Console and storage\n'));
  await userEvent.type(content, 'Agent\n');
  await userEvent.click(screen.getByRole('button', { name: 'Review document publication' }));
  const dialog = within(screen.getByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Publish document' }));
  expect(await dialog.findByText(/Document published/)).toBeTruthy();
  expect(publish).toHaveBeenCalledWith(
    'delivery',
    {
      name: 'Components',
      content: 'Console and storage\nAgent\n',
    },
    expect.any(Object),
  );
  expect(screen.getByRole('combobox', { name: 'Retained Project document' }).textContent).toContain(
    'Components',
  );
  expect(read).toHaveBeenCalledTimes(1);
  expect(
    JSON.stringify(
      queryClient
        .getQueryCache()
        .getAll()
        .map((query) => query.state.data),
    ),
  ).not.toContain('Console and storage');
  expect(JSON.stringify(storage.mock.calls)).not.toContain('Console and storage');
});

it('rejects substituted content with the same artifact id but a different retained digest', async () => {
  const document = retainedDocument();
  vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: document.project_id,
    artifact: { ...document.artifact, content_digest: 'e'.repeat(64) },
    content: 'Substituted private content',
  });
  await openRetainedDocument();
  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(screen.getByRole('textbox', { name: 'Document text' }).textContent).toBe('');
});

it.each(['forbidden', 'not_found'] as const)(
  'shows %s document reads without treating them as empty content',
  async (code) => {
    vi.spyOn(projectDocumentsApi, 'read').mockRejectedValue(
      new ManagementApiError({
        code,
        message: 'Content unavailable',
        requestId: 'read-denied',
        status: code === 'forbidden' ? 403 : 404,
        retryAfterMilliseconds: null,
      }),
    );
    const publish = vi.spyOn(projectDocumentsApi, 'publish');
    await openRetainedDocument();
    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'Document text' }).textContent).toBe('');
    expect(publish).not.toHaveBeenCalled();
  },
);

it('explains that the saved-document picker opens content and does not attach it', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [retainedDocument()],
    next_cursor: null,
  });
  renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  const picker = await screen.findByRole('combobox', { name: 'Retained Project document' });
  expect(picker.textContent).toContain('Choose a document to open');
});

it('distinguishes an expired document from an empty catalog and does not read its bytes', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [{ ...retainedDocument(), expires_at: Date.now() - 1 }],
    next_cursor: null,
  });
  const read = vi.spyOn(projectDocumentsApi, 'read');
  renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Retained Project document' }));
  await userEvent.click(screen.getByRole('option', { name: /Components/ }));
  expect(
    await screen.findByText(
      'This retained document has expired. Publish or select an available version.',
    ),
  ).toBeTruthy();
  expect(
    screen.getByRole('button', { name: 'Open document for viewing or editing' }),
  ).toHaveProperty('disabled', true);
  expect(read).not.toHaveBeenCalled();
});

it('creates a reviewed immutable document from Project settings without attaching it to a pipeline', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  const list = vi
    .spyOn(projectDocumentsApi, 'list')
    .mockResolvedValue({ items: [], next_cursor: null });
  const read = vi.spyOn(projectDocumentsApi, 'read');
  const published = {
    project_id: 'delivery',
    name: 'Components',
    artifact: { artifact_id: 'components-v1', content_digest: 'd'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  };
  const publish = vi.spyOn(projectDocumentsApi, 'publish').mockImplementation(async () => {
    list.mockResolvedValue({ items: [published], next_cursor: null });
    return published;
  });
  renderProjects('/projects/delivery', api);
  await screen.findByRole('heading', { name: 'Delivery' });
  expect(list).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Manage Project documents' }));
  expect(await screen.findByText(/No retained Project documents/)).toBeTruthy();
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Components');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Document text' }),
    'Console and storage\n',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review document publication' }));
  expect(publish).not.toHaveBeenCalled();
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText('Console and storage')).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Publish document' }));
  expect(publish).toHaveBeenCalledWith(
    'delivery',
    { name: 'Components', content: 'Console and storage\n' },
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
  expect(await dialog.findByText(/Document published/)).toBeTruthy();
  expect(read).not.toHaveBeenCalled();
  expect(api.triggerBuild).not.toHaveBeenCalled();
});
