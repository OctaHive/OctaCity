import { useEffect, useRef, useState, type RefObject } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { IssueParentPicker } from './IssueParentPicker';
import { IssuePropertyInput } from './IssuePropertyInput';
import { propertyValue, propertyValueLabel } from './issuePlanningPresentation';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import type { Issue, IssuePlanning, IssuePolicy, IssuePropertyChoices, IssuesApi } from './api';

const ownershipLabels = {
  assignee: 'planning.assignee',
  labels: 'planning.labels',
  due_date: 'planning.dueDate',
  properties: 'planning.custom',
  parent: 'planning.parent',
} as const;

/** A planning draft and its reviewed command exist only while this editor is mounted. */
export function IssuePlanningEditor({
  api,
  issue,
  choices,
  policy,
  ready,
  onRefresh,
  onClose,
  fallbackFocusRef,
}: {
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
  api: IssuesApi;
  issue: Issue;
  choices: IssuePropertyChoices;
  policy: IssuePolicy | undefined;
  ready: boolean;
  onRefresh: () => Promise<unknown>;
  onClose: () => void;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const formRef = useRef<HTMLFormElement>(null);
  useEffect(() => {
    formRef.current
      ?.querySelector<HTMLElement>('[role=combobox]:not(:disabled),input:not(:disabled)')
      ?.focus();
  }, []);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const initial = () =>
    structuredClone(
      issue.planning ?? {
        definition_version: 0,
        assignee: null,
        labels: [],
        due_date: null,
        parent_id: null,
        values: {},
      },
    );
  const fingerprint = JSON.stringify([issue.version, choices, policy]);
  const [baseline, setBaseline] = useState(fingerprint);
  const stale = baseline !== fingerprint;
  const [draft, setDraft] = useState<IssuePlanning>(initial);
  const originalParent = issue.planning?.parent_id;
  const parent = useQuery({
    queryKey: queryKeys.issue(issue.project_id, originalParent ?? ''),
    gcTime: 0,
    enabled: !!originalParent,
    queryFn: ({ signal }) => api.get(issue.project_id, originalParent!, signal),
  });
  const [selectedParentLabel, setSelectedParentLabel] = useState<string | null>(null);
  const parentLabel = draft.parent_id
    ? (selectedParentLabel ??
      (parent.data
        ? `${parent.data.display_key} · ${parent.data.content.title}`
        : t('planning.unavailableValue')))
    : t('planning.noParent');
  const [review, setReview] = useState<{
    request: IssuePlanning;
    version: number;
    summary: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const owned =
    policy?.source_owned.filter((field): field is keyof typeof ownershipLabels =>
      Object.hasOwn(ownershipLabels, field),
    ) ?? [];
  return (
    <section aria-label={t('planning.edit')}>
      <p>{t('issues.memoryDraft')}</p>
      {choices.definitions.definition.labels.some((l) => l.retired) ||
      choices.definitions.definition.properties.some(
        (p) => p.retired || (p.kind.type === 'choice' && p.kind.choices.some((c) => c.retired)),
      ) ? (
        <p>{t('planning.retiredHelp')}</p>
      ) : null}
      {owned.length ? (
        <p>
          {t('issues.ownedFields', {
            fields: owned.map((field) => t(ownershipLabels[field])).join(', '),
          })}
        </p>
      ) : null}
      {stale ? (
        <div role="alert">
          <p>{t('planning.stale')}</p>
          <button
            type="button"
            onClick={() => {
              setDraft(initial());
              setSelectedParentLabel(null);
              setBaseline(fingerprint);
              setError(null);
            }}
          >
            {t('planning.reload')}
          </button>
        </div>
      ) : null}
      <form
        ref={formRef}
        onSubmit={(e) => {
          e.preventDefault();
          if (!ready || stale) return;
          for (const p of choices.definitions.definition.properties) {
            const value = propertyValue(draft.values, p.id);
            if (
              JSON.stringify(value) === JSON.stringify(propertyValue(issue.planning?.values, p.id))
            )
              continue;
            if (
              value?.type === 'text' &&
              p.kind.type === 'text' &&
              new TextEncoder().encode(value.value).length > p.kind.max_bytes
            ) {
              setError(t('issues.byteLimit', { field: p.name, maximum: p.kind.max_bytes }));
              return;
            }
          }
          setError(null);
          setReview({
            request: { ...structuredClone(draft), definition_version: choices.definitions.version },
            version: issue.version,
            summary: [
              t('issues.editBaseline', { version: issue.version }),
              t('planning.definition', { version: choices.definitions.version }),
              `${t('planning.assignee')}: ${draft.assignee ? (choices.assignees.items.find((a) => a.id === draft.assignee)?.name ?? t('planning.assigneeUnavailable')) : t('planning.unassigned')}`,
              `${t('planning.labels')}: ${draft.labels.length ? draft.labels.map((id) => choices.definitions.definition.labels.find((l) => l.id === id)?.name ?? t('planning.unavailableValue')).join(', ') : t('issues.notSet')}`,
              `${t('planning.dueDate')}: ${draft.due_date ?? t('issues.notSet')}`,
              `${t('planning.parent')}: ${parentLabel}`,
              ...choices.definitions.definition.properties.map(
                (p) => `${p.name}: ${propertyValueLabel(propertyValue(draft.values, p.id), p, t)}`,
              ),
            ].join(' · '),
          });
        }}
      >
        <div>
          <span>{t('planning.assignee')}</span>
          <SelectMenu
            ariaLabel={t('planning.assignee')}
            value={draft.assignee ?? ''}
            disabled={
              (!choices.assignees.available && !draft.assignee) || owned.includes('assignee')
            }
            options={[
              { value: '', label: t('planning.unassigned') },
              ...(draft.assignee && !choices.assignees.items.some((a) => a.id === draft.assignee)
                ? [{ value: draft.assignee, label: t('planning.assigneeUnavailable') }]
                : []),
              ...choices.assignees.items.map((a) => ({ value: a.id, label: a.name })),
            ]}
            onValueChange={(assignee) => setDraft({ ...draft, assignee: assignee || null })}
          />
        </div>
        <fieldset disabled={owned.includes('labels')}>
          <legend>{t('planning.labels')}</legend>
          {choices.definitions.definition.labels.map((l) => (
            <label key={l.id}>
              <input
                type="checkbox"
                disabled={
                  !draft.labels.includes(l.id) &&
                  (l.retired || draft.labels.length >= bounds.labelsMaximumCount)
                }
                checked={draft.labels.includes(l.id)}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    labels: e.target.checked
                      ? [...draft.labels, l.id]
                      : draft.labels.filter((id) => id !== l.id),
                  })
                }
              />
              {l.name}
            </label>
          ))}
        </fieldset>
        <label>
          {t('planning.dueDate')}
          <input
            type="date"
            value={draft.due_date ?? ''}
            disabled={owned.includes('due_date')}
            onChange={(e) => setDraft({ ...draft, due_date: e.target.value || null })}
          />
        </label>
        {choices.definitions.definition.properties.map((p) => (
          <div key={p.id}>
            <IssuePropertyInput
              retained={
                JSON.stringify(propertyValue(draft.values, p.id)) ===
                JSON.stringify(propertyValue(issue.planning?.values, p.id))
              }
              definition={p}
              value={propertyValue(draft.values, p.id)}
              disabled={owned.includes('properties') || p.retired}
              onChange={(value) => {
                const values =
                  value === undefined ? { ...draft.values } : { ...draft.values, [p.id]: value };
                if (value === undefined) delete values[p.id];
                setDraft({ ...draft, values });
              }}
            />
            {p.retired && propertyValue(draft.values, p.id) && !owned.includes('properties') ? (
              <button
                type="button"
                onClick={() => {
                  const values = { ...draft.values };
                  delete values[p.id];
                  setDraft({ ...draft, values });
                }}
              >
                {t('planning.clear', { name: p.name })}
              </button>
            ) : null}
          </div>
        ))}
        {error ? <p role="alert">{error}</p> : null}
        <IssueParentPicker
          api={api}
          issue={issue}
          value={draft.parent_id}
          selectedLabel={parentLabel}
          disabled={owned.includes('parent')}
          onChange={(parent_id, label) => {
            setDraft({ ...draft, parent_id });
            setSelectedParentLabel(label);
          }}
        />
        <button ref={reviewButtonRef} disabled={!ready || stale}>
          {t('planning.review')}
        </button>
      </form>
      {review ? (
        <ConfirmedCommand
          fallbackFocusRef={fallbackFocusRef}
          ready={ready && !stale}
          title={t('planning.review')}
          confirmLabel={t('planning.save')}
          consequence={`${review.summary}. ${t('planning.help')}`}
          request={review.request}
          version={review.version}
          returnFocus={reviewButtonRef.current!}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issues(issue.project_id) }]}
          onRefresh={onRefresh}
          execute={async (attempt) => {
            const result = await api.editPlanning(
              issue.project_id,
              issue.id,
              {
                ...attempt.request,
                labels: [...attempt.request.labels],
                values: { ...attempt.request.values },
              },
              requireVersionedMutationHeaders(attempt.headers),
            );
            setSaved(true);
            return result;
          }}
          onClose={() => {
            setReview(null);
            if (saved) onClose();
          }}
          renderSuccess={(result) => (
            <p>{t('issues.saved', { version: result.resource.version })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
