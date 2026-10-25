import { useState } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import {
  QueryFailureNotice,
  QueryLoadingNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import type {
  IssueWorkflowsApi,
  WorkflowResource,
  WorkflowDefault,
} from '../../api/issueWorkflows';
import { useWorkflowSummary } from './workflowSummary';
import { WorkflowCanvas } from './WorkflowCanvas';
import styles from './WorkflowView.module.css';

type Props = {
  api: IssueWorkflowsApi;
  projectId: string;
  selection: WorkflowDefault | undefined;
  refreshSelection: () => Promise<void>;
  onEdit: (resource: WorkflowResource) => void;
};
export function WorkflowCatalog({ api, projectId, selection, refreshSelection, onEdit }: Props) {
  const { t } = usePresentation();
  const listing = useInfiniteQuery({
    queryKey: queryKeys.issueWorkflows(projectId),
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => api.list(projectId, pageParam, signal),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
  const [target, setTarget] = useState<{ id: string; version?: number } | null>(null);
  const [versionInput, setVersionInput] = useState('');
  const selected = useQuery({
    queryKey: queryKeys.issueWorkflow(projectId, target?.id ?? null, target?.version),
    enabled: target !== null,
    queryFn: ({ signal }) => api.get(projectId, target!.id, target?.version, signal),
  });
  return (
    <section className={styles.card} aria-label={t('workflow.catalog')}>
      <h2>{t('workflow.catalog')}</h2>
      {listing.isPending ? (
        <QueryLoadingNotice label={t('workflow.catalog')} />
      ) : listing.data === undefined ? (
        <QueryFailureNotice
          error={listing.error}
          onRetry={listing.refetch}
          title={t('workflow.loadFailure')}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            error={listing.error}
            fetching={listing.isFetching}
            onRetry={listing.refetch}
            label={t('workflow.catalog')}
          />
          <ul>
            {listing.data.pages
              .flatMap((page) => page.items)
              .map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    aria-label={t('workflow.open', { name: item.name })}
                    onClick={() => {
                      setTarget({ id: item.id });
                      setVersionInput('');
                    }}
                  >
                    {item.name} · {t('workflow.version', { version: item.version })}
                  </button>
                </li>
              ))}
          </ul>
          {listing.hasNextPage ? (
            <button
              type="button"
              disabled={listing.isFetching}
              onClick={() => void listing.fetchNextPage()}
            >
              {t('workflow.more')}
            </button>
          ) : null}
        </>
      )}
      {target ? (
        <>
          <div className={styles.actions}>
            <label>
              {t('workflow.versionInput')}
              <input
                type="number"
                min={1}
                step={1}
                value={versionInput}
                onChange={(event) => setVersionInput(event.target.value)}
              />
            </label>
            <button
              type="button"
              disabled={
                !Number.isSafeInteger(Number(versionInput)) ||
                Number(versionInput) < 1 ||
                selected.isFetching
              }
              onClick={() => setTarget({ ...target, version: Number(versionInput) })}
            >
              {t('workflow.loadVersion')}
            </button>
            <button
              type="button"
              disabled={selected.isFetching}
              onClick={() => {
                if (target.version === undefined) void selected.refetch();
                else setTarget({ id: target.id });
              }}
            >
              {t('workflow.loadCurrent')}
            </button>
          </div>
          {selected.isPending ? (
            <QueryLoadingNotice label={t('workflow.catalog')} />
          ) : selected.data === undefined ? (
            <QueryFailureNotice
              error={selected.error}
              onRetry={selected.refetch}
              title={t('workflow.loadFailure')}
            />
          ) : (
            <>
              <QueryBackgroundNotice
                error={selected.error}
                fetching={selected.isFetching}
                onRetry={selected.refetch}
                label={t('workflow.catalog')}
              />
              <WorkflowSelection
                {...{ api, projectId, selection, refreshSelection, onEdit }}
                resource={selected.data}
                ready={selected.error === null && !selected.isFetching}
              />
            </>
          )}
        </>
      ) : null}
    </section>
  );
}

function WorkflowSelection({
  api,
  projectId,
  selection,
  refreshSelection,
  onEdit,
  resource,
  ready,
}: Props & { resource: WorkflowResource; ready: boolean }) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [review, setReview] = useState<{
    resource: WorkflowResource;
    selectionVersion: number;
    focus: HTMLElement;
  } | null>(null);
  const summary = useWorkflowSummary(review?.resource.definition ?? resource.definition);
  return (
    <>
      <h3>{resource.definition.name}</h3>
      <p>{t('workflow.version', { version: resource.version })}</p>
      <WorkflowCanvas definition={resource.definition} />
      <div className={styles.actions}>
        <button type="button" disabled={!ready} onClick={() => onEdit(structuredClone(resource))}>
          {t('workflow.editVersion')}
        </button>
        <button
          type="button"
          disabled={!ready || selection === undefined}
          onClick={(event) => {
            if (selection === undefined) return;
            setReview({
              resource: structuredClone(resource),
              selectionVersion: selection.version,
              focus: event.currentTarget,
            });
          }}
        >
          {t('workflow.use')}
        </button>
      </div>
      {review ? (
        <ConfirmedCommand
          title={t('workflow.select')}
          confirmLabel={t('workflow.select')}
          consequence={`${review.resource.definition.name} · ${t('workflow.version', { version: review.resource.version })}. ${summary}. ${t('workflow.defaultConsequence')}`}
          request={{ workflow_id: review.resource.id, workflow_version: review.resource.version }}
          version={review.selectionVersion}
          returnFocus={review.focus}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issueWorkflowDefault(projectId) }]}
          execute={(attempt) =>
            api.selectDefault(
              projectId,
              structuredClone(attempt.request),
              requireVersionedMutationHeaders(attempt.headers),
            )
          }
          onRefresh={refreshSelection}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('workflow.selected')}</p>}
        />
      ) : null}
    </>
  );
}
