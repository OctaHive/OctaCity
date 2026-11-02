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

it('opens a document as formatted read-only content before explicitly editing it', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  const document = retainedDocument();
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [document], next_cursor: null });
  const read = vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: document.project_id,
    artifact: document.artifact,
    content: '# Project components\n\n- **Console** for operators\n- Storage\n',
  });
  const publish = vi.spyOn(projectDocumentsApi, 'publish');
  renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
  expect(screen.getByRole('button', { name: 'New document' })).toBeTruthy();
  await userEvent.click(await screen.findByRole('button', { name: 'View Components' }));
  const view = within(await screen.findByRole('article', { name: 'Components' }));
  expect(await view.findByRole('heading', { name: 'Project components' })).toBeTruthy();
  expect(view.getByText('Console').tagName).toBe('STRONG');
  expect(view.getAllByRole('listitem')).toHaveLength(2);
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
  expect(view.getByRole('button', { name: 'Edit document' })).toBeTruthy();
  await userEvent.click(view.getByRole('button', { name: 'Back to documents' }));
  expect(await screen.findByRole('button', { name: 'View Components' })).toBeTruthy();
  expect(read).toHaveBeenCalledExactlyOnceWith('delivery', 'components-v1');
  expect(publish).not.toHaveBeenCalled();
});

async function openRetainedDocument() {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [retainedDocument()],
    next_cursor: null,
  });
  const view = renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'View Components' }));
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
  await userEvent.click(screen.getByRole('button', { name: 'Edit document' }));
  let content = screen.getByRole('textbox', { name: 'Document text' });
  await waitFor(() => expect(content.textContent).toBe('Console and storage\n'));
  await userEvent.type(content, 'Agent\n');
  await userEvent.click(screen.getByRole('button', { name: 'Preview' }));
  expect(screen.getByRole('region', { name: 'Document preview' }).textContent).toContain('Agent');
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(screen.getByRole('article', { name: 'Components' }).textContent).not.toContain('Agent');
  await userEvent.click(screen.getByRole('button', { name: 'Edit document' }));
  content = screen.getByRole('textbox', { name: 'Document text' });
  await userEvent.type(content, 'Agent\n');
  await userEvent.click(screen.getByRole('button', { name: 'Publish changes' }));
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
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
  expect(screen.getByRole('article', { name: 'Components' }).textContent).toContain('Agent');
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
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
  expect(screen.queryByRole('article', { name: 'Components' })).toBeNull();
});

it('keeps Markdown links inert for data and script URLs while preserving web and relative links', async () => {
  const document = retainedDocument();
  vi.spyOn(projectDocumentsApi, 'read').mockResolvedValue({
    project_id: document.project_id,
    artifact: document.artifact,
    content:
      '[SVG](data:image/svg+xml;base64,PHN2Zy8+)\n\n[Script](javascript:alert(1))\n\n[Web](https://example.org/goals)\n\n[Local](./components)',
  });
  await openRetainedDocument();
  const view = within(await screen.findByRole('article', { name: 'Components' }));
  expect(view.queryByRole('link', { name: 'SVG' })).toBeNull();
  expect(view.queryByRole('link', { name: 'Script' })).toBeNull();
  expect(view.getByRole('link', { name: 'Web' }).getAttribute('href')).toBe(
    'https://example.org/goals',
  );
  expect(view.getByRole('link', { name: 'Local' }).getAttribute('href')).toBe('./components');
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
    expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
    expect(publish).not.toHaveBeenCalled();
  },
);

it('lists saved documents for explicit viewing without opening an empty editor', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({
    items: [retainedDocument()],
    next_cursor: null,
  });
  renderProjects('/projects/delivery', api);
  await userEvent.click(await screen.findByRole('button', { name: 'Manage Project documents' }));
  expect(await screen.findByRole('button', { name: 'View Components' })).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Document text' })).toBeNull();
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
  expect(
    await screen.findByText(
      'This retained document has expired. Publish or select an available version.',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'View Components' })).toHaveProperty('disabled', true);
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
  expect(await screen.findByText(/No documents yet/)).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'New document' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'Components');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Document text' }),
    'Console and storage\n',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Publish document' }));
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
