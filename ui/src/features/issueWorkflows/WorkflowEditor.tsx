import { useState, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { WorkflowCanvas } from './WorkflowCanvas';
import { useWorkflowSummary } from './workflowSummary';
import { ISSUE_WORKFLOW_CONSTRAINTS as bounds } from '../../../.generated/api/issue-workflow';
import type {
  IssueWorkflowsApi,
  WorkflowResource,
  WorkflowDefinition,
} from '../../api/issueWorkflows';
import styles from './WorkflowView.module.css';

/** Drafts and validation receipts live only in this mounted editor. Publication never changes the default. */
export function WorkflowEditor({
  api,
  projectId,
  baseline,
  creating = false,
}: {
  api: IssueWorkflowsApi;
  projectId: string;
  baseline: WorkflowResource;
  creating?: boolean;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<WorkflowDefinition>(() =>
    creating
      ? { name: '', statuses: [], initial_status_id: '', terminal_status_ids: [], transitions: [] }
      : structuredClone(baseline.definition),
  );
  const [version, setVersion] = useState<number | undefined>(
    creating ? undefined : baseline.version,
  );
  const validationButtonRef = useRef<HTMLButtonElement>(null);
  const [proof, setProof] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{
    definition: WorkflowDefinition;
    version: number | undefined;
    focus: HTMLElement;
  } | null>(null);
  const signature = JSON.stringify(draft);
  const latestRef = useRef(signature);
  latestRef.current = signature;
  const summary = useWorkflowSummary(review?.definition ?? draft);
  const [baselineReady, setBaselineReady] = useState(true);
  const invalidNames = [
    draft.name,
    ...draft.statuses.map((item) => item.name),
    ...draft.transitions.map((item) => item.name),
  ].some(
    (name) =>
      !name.trim() ||
      name !== name.trim() ||
      /[\p{Cc}]/u.test(name) ||
      new TextEncoder().encode(name).length > bounds.nameMaximumBytes,
  );
  const localError = invalidNames
    ? t('workflow.nameBound', { bytes: bounds.nameMaximumBytes })
    : draft.statuses.length < bounds.minimumStatuses
      ? t('workflow.statusRequired')
      : null;
  async function validate() {
    if (localError) return;
    if (!baselineReady) {
      try {
        const head = await api.get(projectId, baseline.id);
        setVersion(head.version);
        setBaselineReady(true);
      } catch (failure) {
        setError(failure instanceof Error ? failure : new Error());
        return;
      }
    }
    const captured = signature;
    setPending(true);
    setProof(null);
    setError(null);
    try {
      await api.validate(projectId, structuredClone(draft));
      if (latestRef.current === captured) setProof(captured);
    } catch (failure) {
      if (latestRef.current === captured)
        setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  return (
    <section className={styles.card} aria-label={t('workflow.draft')}>
      <h2>{t('workflow.draft')}</h2>
      <p>
        {version === undefined
          ? t('workflow.newDefinition')
          : t('workflow.baseVersion', { version })}
      </p>
      <WorkflowCanvas definition={draft} onChange={setDraft} />
      {localError ? <p role="alert">{localError}</p> : null}
      {error ? (
        <QueryFailureNotice
          error={error}
          title={t('workflow.validationFailure')}
          onRetry={validate}
        />
      ) : null}
      <div className={styles.actions}>
        <button
          type="button"
          ref={validationButtonRef}
          disabled={pending}
          onClick={() => void validate()}
        >
          {t('workflow.validate')}
        </button>
        <button
          type="button"
          disabled={pending || !baselineReady || localError !== null || proof !== signature}
          onClick={(event) =>
            setReview({ definition: structuredClone(draft), version, focus: event.currentTarget })
          }
        >
          {t('workflow.review')}
        </button>
      </div>
      {review ? (
        <ConfirmedCommand
          title={t('workflow.review')}
          confirmLabel={t('workflow.publish')}
          consequence={`${review.definition.name} · ${review.version === undefined ? t('workflow.newDefinition') : t('workflow.baseVersion', { version: review.version })}. ${summary}. ${t('workflow.publicationConsequence')}`}
          request={review.definition}
          {...(review.version === undefined ? {} : { version: review.version })}
          // A stale refresh disables the review trigger until the draft is validated again.
          returnFocus={
            proof === signature ? review.focus : (validationButtonRef.current ?? review.focus)
          }
          queryClient={queryClient}
          invalidations={[
            { queryKey: queryKeys.issueWorkflows(projectId) },
            { queryKey: queryKeys.issueWorkflow(projectId, baseline.id), exact: true },
          ]}
          execute={async (attempt) => {
            const result = await (review.version === undefined
              ? api.create(
                  projectId,
                  {
                    id: baseline.id,
                    definition: structuredClone(attempt.request) as WorkflowDefinition,
                  },
                  attempt.headers,
                )
              : api.publish(
                  projectId,
                  baseline.id,
                  structuredClone(attempt.request) as WorkflowDefinition,
                  requireVersionedMutationHeaders(attempt.headers),
                ));
            setVersion(result.resource.version);
            return result;
          }}
          onRefresh={async () => {
            setProof(null);
            setBaselineReady(false);
            try {
              const head = await api.get(projectId, baseline.id);
              setVersion(head.version);
              setBaselineReady(true);
              setError(null);
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            }
          }}
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <p>{t('workflow.published', { version: result.resource.version })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
