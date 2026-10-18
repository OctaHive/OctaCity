import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { ManagementApiError, redactManagementError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import type { FactorySourcesApi, SourceIntent } from './sourceApi';
import { SourceAdmissions } from '../../shared/SourceAdmissions';

/** Proofs, previews and confirmed snapshots live only in this mounted editor. */
export function SourcePublication({
  intent,
  summary,
  api,
  onRefresh,
  onPublished,
}: {
  intent: SourceIntent | null;
  summary: string;
  api: FactorySourcesApi;
  onRefresh: () => Promise<void>;
  onPublished: (result: Awaited<ReturnType<FactorySourcesApi['publish']>>) => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const signature = JSON.stringify(intent);
  const latestRef = useRef(signature);
  latestRef.current = signature;
  const [proof, setProof] = useState<string | null>(null);
  const [preview, setPreview] = useState<{
    signature: string;
    value: Awaited<ReturnType<FactorySourcesApi['preview']>>;
  } | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshRequired, setRefreshRequired] = useState(false);
  const [review, setReview] = useState<{
    intent: SourceIntent;
    summary: string;
    returnFocus: HTMLElement;
  } | null>(null);
  const group = !!intent?.group;
  async function read(kind: 'validate' | 'preview') {
    if (!intent || pending || refreshRequired) return;
    const captured = signature;
    setPending(true);
    setError(null);
    if (kind === 'validate') setProof(null);
    try {
      if (kind === 'validate') {
        await api.validate(intent);
        if (latestRef.current === captured) setProof(captured);
      } else {
        const value = await api.preview(intent);
        if (latestRef.current === captured) setPreview({ signature: captured, value });
      }
    } catch {
      if (latestRef.current === captured) setError(t('factory.sourceFailure'));
    } finally {
      setPending(false);
    }
  }
  async function execute(attempt: ConfirmedMutationAttempt<SourceIntent>) {
    try {
      const result = await api.publish(
        structuredClone(attempt.request) as SourceIntent,
        attempt.headers,
      );
      setProof(null);
      onPublished(result);
      return result;
    } catch (failure) {
      if (
        failure instanceof ManagementApiError &&
        (failure.code === 'precondition_failed' || failure.code === 'conflict')
      ) {
        setRefreshRequired(true);
        setProof(null);
      }
      // Never display provider or transport text in a draft/confirmation.
      throw redactManagementError(failure, t('factory.sourceFailure'));
    }
  }
  async function refresh() {
    setProof(null);
    setRefreshRequired(true);
    try {
      await onRefresh();
      setRefreshRequired(false);
      setError(null);
    } catch {
      setError(t('factory.sourceFailure'));
    }
  }
  return (
    <section aria-label={t('factory.sourceValidation')}>
      {error ? <p role="alert">{error}</p> : null}
      {refreshRequired ? (
        <p role="alert">
          {t('factory.sourceStale')}{' '}
          <button type="button" onClick={() => void refresh()}>
            {t('factory.refreshSource')}
          </button>
        </p>
      ) : null}
      <button
        type="button"
        disabled={!intent || pending || review !== null || refreshRequired}
        onClick={() => void read('preview')}
      >
        {t('factory.previewSource')}
      </button>
      <button
        type="button"
        disabled={!intent || pending || review !== null || refreshRequired}
        onClick={() => void read('validate')}
      >
        {t('factory.validateInput')}
      </button>
      <button
        type="button"
        disabled={!intent || proof !== signature || pending || refreshRequired || review !== null}
        onClick={(event) =>
          intent &&
          setReview({ intent: structuredClone(intent), summary, returnFocus: event.currentTarget })
        }
      >
        {t('factory.reviewInput')}
      </button>
      {preview?.signature === signature ? (
        <section aria-label={t('factory.sourcePreview')}>
          <p>{t('factory.previewSourceHelp')}</p>
          {preview.value.failure ? (
            <p role="alert">
              {t('factory.previewUnavailable')} · {preview.value.failure}
            </p>
          ) : (
            <>
              {!preview.value.candidates.length ? <p>{t('factory.previewEmpty')}</p> : null}
              <ul>
                {preview.value.candidates.map((item) => (
                  <li key={item.native_correlation}>
                    {item.summary.display_id} · {item.summary.title} ·{' '}
                    {t(`factory.preview.${item.disposition}`)}
                    <ul>
                      {item.summary.selections.map((selection) => (
                        <li key={selection.input_id}>
                          {preview.value.selectors.find(
                            (selector) => selector.input_id === selection.input_id,
                          )?.binding.configuration.identity ?? selection.input_id}
                          :{' '}
                          {selection.reasons
                            .map((reason) => t(`factory.previewReason.${reason}`))
                            .join(', ') || t('factory.preview.selected')}
                        </li>
                      ))}
                    </ul>
                    <SourceAdmissions admissions={item.admissions} />
                  </li>
                ))}
              </ul>
              {preview.value.has_more ? <p>{t('factory.previewMore')}</p> : null}
              {preview.value.admissions_truncated ? (
                <p>{t('factory.admissionsTruncated')}</p>
              ) : null}
            </>
          )}
        </section>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t(group ? 'factory.publishGroup' : 'factory.publishInput')}
          confirmLabel={t('factory.confirmInput')}
          consequence={review.summary}
          request={review.intent}
          returnFocus={review.returnFocus}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.factorySourceInputs(review.intent.project_id) }]}
          execute={execute}
          onRefresh={refresh}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('factory.sourcePublished')}</p>}
        />
      ) : null}
    </section>
  );
}
