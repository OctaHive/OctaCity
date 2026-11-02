import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { QueryFailureNotice, queryFailureKind } from '../../shared/QueryStateNotice';
import { ManagementApiError } from '../../api/client';
import { constraintSourceLabels } from './ConstraintReview';
import { focusFactoryField } from './fieldFocus';
import { AccessPolicyFields } from './AccessPolicyFields';
import { PolicyPermissionDetails, PolicySummary } from './PolicySummary';
import type { FactoryPolicyApi, PolicyRecord, PolicyRef } from './policyApi';

type Draft = components['schemas']['FactoryProjectPolicyDraft'];
/** The draft and its original precondition stay in memory until a reviewed command succeeds. */
export function PolicyEditor({
  project,
  api,
  initial,
  onCancel,
  onPublished,
}: {
  project: string;
  api: FactoryPolicyApi;
  initial: Draft;
  onCancel: () => void;
  onPublished: (record: PolicyRecord) => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [draft, setDraft] = useState(() => structuredClone(initial));
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{
    request: Draft;
    validation: components['schemas']['FactoryProjectPolicyValidation'];
    target: HTMLElement;
  } | null>(null);
  const [current, setCurrent] = useState<PolicyRecord | null>(null);
  const [published, setPublished] = useState<PolicyRecord | null>(null);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const stale = draft.expected_current !== null && queryFailureKind(error) === 'conflict';
  function updateDraft(next: Draft) {
    setDraft(next);
    setError(null);
  }
  async function validate() {
    if (!reviewButtonRef.current) return;
    const request = structuredClone(draft);
    const target = reviewButtonRef.current;
    setPending(true);
    setError(null);
    try {
      const result = await api.validate(project, request);
      setReview({ request, validation: result, target });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  async function refresh() {
    setPending(true);
    setError(null);
    try {
      let after: PolicyRef | null = null;
      do {
        const page = await api.list(project, after);
        const current = page.items.find(
          (item) => item.reference.kind === draft.kind && item.reference.reference.id === draft.id,
        );
        if (current) {
          const latest = await api.get(project, current.reference);
          setCurrent(latest.policy);
          setDraft({ ...draft, expected_current: latest.policy.policy.reference });
          await client.invalidateQueries({ queryKey: queryKeys.factoryPolicies(project) });
          return;
        }
        after = page.next_cursor;
      } while (after);
      throw new Error(t('policy.unavailable'));
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  return (
    <section aria-label={t('policy.editor')}>
      <h3>{t('policy.editor')}</h3>
      <p>{t('factory.memoryDraft')}</p>
      {draft.expected_current ? (
        <p>{t('policy.editingVersion', { version: draft.expected_current.version })}</p>
      ) : null}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void validate();
        }}
      >
        <fieldset disabled={pending || review !== null}>
          <label>
            {t('policy.name')}
            <input
              required
              value={draft.definition.name}
              onChange={(event) =>
                updateDraft({
                  ...draft,
                  definition: { ...draft.definition, name: event.target.value },
                } as Draft)
              }
            />
          </label>
          {draft.kind === 'launch' ? (
            <>
              <label>
                <input
                  type="checkbox"
                  checked={draft.definition.allow_manual}
                  onChange={(event) =>
                    updateDraft({
                      ...draft,
                      definition: { ...draft.definition, allow_manual: event.target.checked },
                    })
                  }
                />
                {t('policy.allowManual')}
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={draft.definition.allow_automatic_source}
                  onChange={(event) =>
                    updateDraft({
                      ...draft,
                      definition: {
                        ...draft.definition,
                        allow_automatic_source: event.target.checked,
                      },
                    })
                  }
                />
                {t('policy.allowAutomatic')}
              </label>
              <p>{t('policy.sourceHelp')}</p>
            </>
          ) : (
            <AccessPolicyFields
              project={project}
              api={api}
              value={draft.definition.permissions}
              onChange={(permissions) =>
                updateDraft({ ...draft, definition: { ...draft.definition, permissions } })
              }
            />
          )}
          <button ref={reviewButtonRef} type="submit">
            {t('policy.review')}
          </button>
          <button type="button" onClick={onCancel}>
            {t('policy.cancel')}
          </button>
        </fieldset>
      </form>
      {error instanceof ManagementApiError
        ? error.validationIssues.map((issue) => (
            <div key={issue.field} role="alert">
              <button type="button" onClick={() => focusFactoryField(issue.field)}>
                {t('policy.requestedAccess')} ·{' '}
                {issue.ceiling ? t(constraintSourceLabels[issue.ceiling]) : t('policy.review')}
              </button>
              <p>{t('factory.accessExceedsPolicyLimit')}</p>
            </div>
          ))
        : null}
      {error ? (
        <QueryFailureNotice
          title={t('policy.review')}
          error={error}
          onRetry={stale ? refresh : validate}
          {...(stale ? { retryLabel: t('command.refreshCurrent') } : {})}
        />
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('policy.review')}
          confirmLabel={t('policy.publish')}
          consequence={t('policy.consequence')}
          renderReview={
            <>
              {current ? (
                <PolicyReviewSection label={t('policy.currentSaved')} record={current} />
              ) : null}
              <PolicyReviewSection label={t('policy.proposed')} record={review.validation.policy} />
              {review.validation.installed_ceiling ? (
                <section aria-label={t('policy.validatedInstallation')}>
                  <h3>{t('policy.validatedInstallation')}</h3>
                  <p>
                    {review.validation.installed_ceiling.alias} ·{' '}
                    {review.validation.installed_ceiling.reference.version}
                  </p>
                  {review.validation.installed_ceiling.permissions ? (
                    <PolicyPermissionDetails
                      permissions={review.validation.installed_ceiling.permissions}
                    />
                  ) : (
                    <p>{t('policy.installationDetailsUnavailable')}</p>
                  )}
                </section>
              ) : null}
            </>
          }
          {...(draft.expected_current ? { onRefresh: refresh } : {})}
          request={review.request}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryPolicies(project) },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const result = await api.publish(
              project,
              structuredClone(attempt.request) as Draft,
              attempt.headers,
            );
            setPublished(result.policy);
            return result;
          }}
          onClose={() => {
            setReview(null);
            if (published) onPublished(published);
          }}
          renderSuccess={() => <p>{t('policy.published')}</p>}
        />
      ) : null}
    </section>
  );
}

function PolicyReviewSection({ label, record }: { label: string; record: PolicyRecord }) {
  return (
    <section aria-label={label}>
      <h3>{label}</h3>
      <p>
        {record.policy.definition.name} · v{record.policy.reference.version}
      </p>
      <PolicySummary record={record} />
    </section>
  );
}
