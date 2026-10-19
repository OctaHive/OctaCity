import { Fragment, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import { propertyValue, propertyValueLabel } from './issuePlanningPresentation';
import { IssuePlanningEditor } from './IssuePlanningEditor';
import type { IssuePolicy, Issue, IssuesApi } from './api';

/** Current planning is independent of the selected historical execution/requirements. */
export function IssueProperties({
  api,
  issue,
  policy,
  ready,
  onRefresh,
}: {
  api: IssuesApi;
  issue: Issue;
  policy: IssuePolicy | undefined;
  ready: boolean;
  onRefresh: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const [editing, setEditing] = useState(false);
  const choices = useQuery({
    queryKey: queryKeys.issuePropertyChoices(issue.project_id),
    gcTime: 0,
    queryFn: ({ signal }) => api.planningChoices(issue.project_id, signal),
  });
  const savedVersion = issue.planning?.definition_version ?? 0;
  const needsRetained =
    savedVersion > 0 && !!choices.data && savedVersion !== choices.data.definitions.version;
  const retained = useQuery({
    queryKey: queryKeys.issuePropertyDefinitions(issue.project_id, savedVersion),
    gcTime: 0,
    enabled: needsRetained,
    queryFn: ({ signal }) => api.propertyDefinitions(issue.project_id, savedVersion, signal),
  });
  if (!choices.data)
    return choices.isPending ? (
      <QueryLoadingNotice label={t('issues.properties')} />
    ) : (
      <QueryFailureNotice
        title={t('issues.properties')}
        error={choices.error}
        onRetry={choices.refetch}
      />
    );
  const definitions = needsRetained ? retained.data : choices.data.definitions;
  const { assignees } = choices.data;
  const plan = issue.planning;
  const planningReady =
    ready &&
    !choices.isFetching &&
    !choices.error &&
    (!needsRetained || (!!retained.data && !retained.isFetching && !retained.error));
  return (
    <>
      <QueryBackgroundNotice
        label={t('issues.properties')}
        error={choices.error}
        fetching={choices.isFetching}
        onRetry={choices.refetch}
      />
      {needsRetained && retained.data ? (
        <QueryBackgroundNotice
          label={t('planning.savedDefinition', { version: savedVersion })}
          error={retained.error}
          fetching={retained.isFetching}
          onRetry={retained.refetch}
        />
      ) : null}
      <p>{t('planning.help')}</p>
      <button ref={editButtonRef} disabled={!planningReady} onClick={() => setEditing(!editing)}>
        {t('planning.edit')}
      </button>
      {editing ? (
        <IssuePlanningEditor
          fallbackFocusRef={editButtonRef}
          api={api}
          issue={issue}
          choices={choices.data}
          policy={policy}
          ready={planningReady}
          onRefresh={onRefresh}
          onClose={() => setEditing(false)}
        />
      ) : null}
      {!assignees.available ? <p>{t('planning.directoryMissing')}</p> : null}
      {/* Retained-catalog state replaces only the read view, never the mounted draft/command. */}
      {definitions ? (
        <>
          <dl>
            <dt>{t('planning.assignee')}</dt>
            <dd>
              {plan?.assignee
                ? (assignees.items.find((p) => p.id === plan.assignee)?.name ??
                  t('planning.assigneeUnavailable'))
                : t('planning.unassigned')}
            </dd>
            <dt>{t('planning.dueDate')}</dt>
            <dd>{plan?.due_date ?? t('issues.notSet')}</dd>
            <dt>{t('planning.labels')}</dt>
            <dd>
              {plan?.labels.length
                ? plan.labels
                    .map(
                      (id) =>
                        definitions.definition.labels.find((c) => c.id === id)?.name ??
                        t('planning.unavailableValue'),
                    )
                    .join(', ')
                : t('issues.notSet')}
            </dd>
            {definitions.definition.properties.map((p) => (
              <Fragment key={p.id}>
                <dt>{p.name}</dt>
                <dd>{propertyValueLabel(propertyValue(plan?.values, p.id), p, t)}</dd>
              </Fragment>
            ))}
          </dl>
          {!definitions.definition.properties.length ? <p>{t('planning.empty')}</p> : null}
        </>
      ) : retained.isPending ? (
        <QueryLoadingNotice label={t('issues.properties')} />
      ) : (
        <QueryFailureNotice
          title={t('issues.properties')}
          error={retained.error}
          onRetry={retained.refetch}
        />
      )}
      <p>{t('planning.definition', { version: choices.data.definitions.version })}</p>
      {needsRetained && retained.data ? (
        <p>{t('planning.savedDefinition', { version: savedVersion })}</p>
      ) : null}
    </>
  );
}
