import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { catalogLabels } from './catalogLabels';
import type { FactoryApi } from './api';

type S = components['schemas'];

/** Examples open the same draft editor as an empty graph; execution is configured there. */
export function FactoryExample({
  api,
  projectId,
  flowId,
  onOpen,
}: {
  api: FactoryApi;
  projectId: string;
  flowId: string;
  onOpen: (draft: S['FactoryFlowDefinitionDraft']) => void;
}) {
  const { t } = usePresentation();
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<{ error: Error; preset: S['FactoryPresetSummary'] } | null>(
    null,
  );
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), []);
  const query = useCursorPage(['factory', 'examples', projectId], async (cursor, signal) => {
    const page = await api.listPresets(projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((item) => ({ ...item, id: sourceReferenceKey(item.reference) })),
    };
  });
  const open = async (preset: S['FactoryPresetSummary']) => {
    requestRef.current?.abort();
    const controller = new AbortController();
    requestRef.current = controller;
    setLoading(sourceReferenceKey(preset.reference));
    setError(null);
    try {
      const draft = await api.openPresetDraft(
        projectId,
        { flow_id: flowId, preset: preset.reference },
        controller.signal,
      );
      if (!controller.signal.aborted) onOpen(draft);
    } catch (failure) {
      if (!controller.signal.aborted)
        setError({ error: failure instanceof Error ? failure : new Error(), preset });
    } finally {
      if (!controller.signal.aborted) setLoading(null);
    }
  };
  return (
    <PagedExplorerBranch
      query={query}
      empty={t('factory.noExamples')}
      labels={catalogLabels(t('factory.examples'))}
    >
      {(items) => (
        <>
          {items
            .filter((item) => item.editor)
            .map((item) => (
              <button
                key={item.id}
                type="button"
                disabled={loading === item.id}
                onClick={() => void open(item)}
              >
                {item.editor?.name} · {item.reference.version}
              </button>
            ))}
          {error ? (
            <QueryFailureNotice
              title={t('factory.examples')}
              error={error.error}
              onRetry={() => void open(error.preset)}
            />
          ) : null}
        </>
      )}
    </PagedExplorerBranch>
  );
}
