import { queryKeys } from '../../app/query';
import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { issuePath } from '../../app/routes';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { Issue, IssueContent, IssuePolicy, IssuesApi } from './api';
import styles from './Issues.module.css';

const contentFields = ['title', 'description', 'acceptance', 'priority'] as const;

/** Unfinished requirements and confirmed intent stay in mounted component memory only. */
export function IssueEditor({
  api,
  projectId,
  issue,
  policy,
  ready = true,
  onRefresh,
}: {
  api: IssuesApi;
  projectId: string;
  issue?: Issue;
  policy?: IssuePolicy;
  ready?: boolean;
  onRefresh?: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const formRef = useRef<HTMLFormElement>(null);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const [baseline, setBaseline] = useState(() =>
    issue ? { version: issue.version, content: structuredClone(issue.content) } : null,
  );
  useEffect(() => {
    formRef.current
      ?.querySelector<HTMLElement>('input:not(:disabled), textarea:not(:disabled)')
      ?.focus();
  }, []);
  const owned = policy?.source_owned.filter((field) => field !== 'status') ?? [];
  const [draft, setDraft] = useState<IssueContent>(() =>
    issue
      ? structuredClone(issue.content)
      : { title: '', description: '', acceptance: null, priority: null },
  );
  const stale =
    !!issue &&
    !!baseline &&
    (issue.version !== baseline.version ||
      owned.some((field) => draft[field] !== issue.content[field]));
  const [error, setError] = useState<string | null>(null);
  const [review, setReview] = useState<{
    content: IssueContent;
    focus: HTMLElement;
    version?: number;
  } | null>(null);
  return (
    <section className={styles.panel}>
      <h2>{t(issue ? 'issues.edit' : 'issues.create')}</h2>
      <p>{t('issues.memoryDraft')}</p>
      {baseline ? <p>{t('issues.editBaseline', { version: baseline.version })}</p> : null}
      {stale ? (
        <section role="alert">
          <p>{t('issues.draftChanged', { version: issue.version })}</p>
          <details>
            <summary>{t('issues.latestChanges')}</summary>
            <dl>
              {contentFields
                .filter(
                  (field) =>
                    baseline.content[field] !== issue.content[field] ||
                    (owned.includes(field) && draft[field] !== issue.content[field]),
                )
                .map((field) => (
                  <div key={field}>
                    <dt>{t(`issues.${field}`)}</dt>
                    <dd className={styles.prose}>{issue.content[field] ?? t('issues.notSet')}</dd>
                  </div>
                ))}
            </dl>
          </details>
        </section>
      ) : null}
      {owned.length ? (
        <p>
          {t('issues.ownedFields', {
            fields: owned.map((field) => t(`issues.${field}`)).join(', '),
          })}
        </p>
      ) : null}
      <form
        ref={formRef}
        onSubmit={(event) => {
          event.preventDefault();
          if (!ready) return;
          if (stale) {
            // Only explicit reconciliation can advance the draft baseline.
            const previousContent = baseline.content;
            const currentContent = issue.content;
            function value<Key extends keyof IssueContent>(field: Key): IssueContent[Key] {
              return !owned.includes(field) && draft[field] !== previousContent[field]
                ? draft[field]
                : currentContent[field];
            }
            setDraft({
              title: value('title'),
              description: value('description'),
              acceptance: value('acceptance') ?? null,
              priority: value('priority') ?? null,
            });
            setBaseline({ version: issue.version, content: structuredClone(issue.content) });
            setError(null);
            return;
          }
          for (const [field, maximum] of [
            ['title', bounds.titleMaximumBytes],
            ['description', bounds.descriptionMaximumBytes],
            ['acceptance', bounds.acceptanceMaximumBytes],
          ] as const) {
            if (new TextEncoder().encode(draft[field] ?? '').length > maximum) {
              setError(t('issues.byteLimit', { field: t(`issues.${field}`), maximum }));
              return;
            }
          }
          if (
            draft.title.trim() !== draft.title ||
            !draft.title.trim() ||
            /[\r\n\t]/u.test(draft.title)
          ) {
            setError(t('issues.invalidTitle'));
            return;
          }
          setError(null);
          setReview({
            content: structuredClone(draft),
            ...(baseline ? { version: baseline.version } : {}),
            focus: reviewButtonRef.current!,
          });
        }}
      >
        <label>
          {t('issues.title')}
          <input
            required
            disabled={policy?.source_owned.includes('title')}
            value={draft.title}
            onChange={(e) => setDraft({ ...draft, title: e.target.value })}
          />
        </label>
        <label>
          {t('issues.description')}
          <textarea
            disabled={policy?.source_owned.includes('description')}
            value={draft.description}
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
          />
        </label>
        <label>
          {t('issues.acceptance')}
          <textarea
            disabled={policy?.source_owned.includes('acceptance')}
            value={draft.acceptance ?? ''}
            onChange={(e) => setDraft({ ...draft, acceptance: e.target.value || null })}
          />
        </label>
        <label>
          {t('issues.priority')}
          <input
            type="number"
            min={bounds.priorityMinimum}
            max={bounds.priorityMaximum}
            disabled={policy?.source_owned.includes('priority')}
            value={draft.priority ?? ''}
            onChange={(e) =>
              setDraft({
                ...draft,
                priority: e.target.value === '' ? null : e.target.valueAsNumber,
              })
            }
          />
        </label>
        <button type="submit" ref={reviewButtonRef} disabled={!ready}>
          {t(stale ? 'issues.reconcileDraft' : 'issues.review')}
        </button>
      </form>
      {error ? <p role="alert">{error}</p> : null}
      {review ? (
        <ConfirmedCommand
          title={t('issues.review')}
          confirmLabel={t(issue ? 'issues.save' : 'issues.create')}
          consequence={`${review.version ? t('issues.editBaseline', { version: review.version }) + '. ' : ''}${t('issues.title')}: ${review.content.title}. ${t('issues.description')}: ${review.content.description || t('issues.noDescription')} ${t('issues.acceptance')}: ${review.content.acceptance || t('issues.noAcceptance')} ${t('issues.priority')}: ${review.content.priority ?? t('issues.notSet')}. ${t('issues.saveConsequence')}`}
          request={review.content}
          {...(review.version === undefined ? {} : { version: review.version })}
          {...(onRefresh ? { onRefresh } : {})}
          returnFocus={review.focus}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issues(projectId) }]}
          execute={async (attempt) => {
            const result = await (issue
              ? api.edit(
                  projectId,
                  issue.id,
                  structuredClone(attempt.request),
                  requireVersionedMutationHeaders(attempt.headers),
                )
              : api.create(projectId, structuredClone(attempt.request), attempt.headers));
            if (issue)
              setBaseline({
                version: result.resource.version,
                content: structuredClone(result.resource.content),
              });
            return result;
          }}
          onClose={() => {
            setReview(null);
          }}
          renderSuccess={(result) =>
            issue ? (
              <p>{t('issues.saved', { version: result.resource.version })}</p>
            ) : (
              <Link to={issuePath(projectId, result.resource.id)}>
                {t('issues.open', { key: result.resource.display_key })}
              </Link>
            )
          }
        />
      ) : null}
    </section>
  );
}
