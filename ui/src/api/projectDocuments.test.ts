import { afterEach, expect, it, vi } from 'vitest';
import { createManagementApiClient } from './client';
import { createProjectDocumentsApi, readExactProjectDocument } from './projectDocuments';

afterEach(() => vi.unstubAllGlobals());

it('rejects a mismatched document identity at the document read boundary', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const artifact = {
    artifact_id: 'other-document',
    content_digest: 'a'.repeat(64),
    encoded_size: 7,
  };
  const api = createProjectDocumentsApi(
    createManagementApiClient(
      vi.fn(async () =>
        Response.json({
          project_id: 'project',
          artifact,
          expires_at: 4102444800000,
          content: 'private',
        }),
      ),
    ),
  );
  await expect(api.read('project', 'expected-document')).rejects.toThrow(
    'Project document response is invalid',
  );
  await expect(
    readExactProjectDocument(api, 'project', { ...artifact, encoded_size: 8 }),
  ).rejects.toThrow('Project document response is invalid');
});

it('rejects an exact preview response without an authoritative retention deadline', async () => {
  vi.stubGlobal('location', { origin: 'https://console.example' });
  const artifact = {
    artifact_id: 'retained-document',
    content_digest: 'a'.repeat(64),
    encoded_size: 7,
  };
  const api = createProjectDocumentsApi(
    createManagementApiClient(
      vi.fn(async () => Response.json({ project_id: 'project', artifact, content: 'private' })),
    ),
  );
  await expect(readExactProjectDocument(api, 'project', artifact)).rejects.toThrow(
    'Project document response is invalid',
  );
});
