import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { vi } from 'vitest';
import type { components } from '../../.generated/api/schema';
import { configurationFixture } from './factory';
import { createConsoleQueryClient } from '../../src/app/query';
import { projectDocumentsApi } from '../../src/api/projectDocuments';
import { FlowGraphEditor } from '../../src/features/factory/FlowGraphEditor';
import { emptyFlowDraft } from '../../src/features/factory/emptyFlowDraft';

export function manualFlowEditorFixture(
  configure?: (draft: components['schemas']['FactoryFlowDefinitionDraft']) => void,
) {
  const flow = configurationFixture().configuration.flow;
  const document = (id: string, name: string) => ({
    project_id: 'delivery',
    name,
    artifact: { artifact_id: id, content_digest: 'a'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  });
  const documents = [
    document('goals-v1', 'Goals'),
    document('components-v1', 'Components'),
    document('priorities-v1', 'Priorities'),
  ];
  const list = vi
    .spyOn(projectDocumentsApi, 'list')
    .mockResolvedValue({ items: documents, next_cursor: null });
  const read = vi.spyOn(projectDocumentsApi, 'read').mockImplementation(async (_, id) => ({
    project_id: 'delivery',
    artifact: documents.find((item) => item.artifact.artifact_id === id)!.artifact,
    content: `Exact ${id} content`,
  }));
  let observed = emptyFlowDraft();
  const original = flow.closure.definitions[0]!.nodes[0]!;
  observed = {
    ...observed,
    data_schemas: flow.data_schemas,
    root: {
      ...observed.root,
      entry: 'assess',
      nodes: ['assess', 'classify'].map((key) => ({
        ...original,
        key,
        kind: 'reasoning',
        build: null,
        action: null,
        gate: null,
        profile_selection: null,
        context_binding: null,
      })),
    },
  };
  configure?.(observed);
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        documents={{ api: projectDocumentsApi, projectId: 'delivery' }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  const queryClient = createConsoleQueryClient();
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return { documents, list, read, queryClient, draft: () => observed };
}
