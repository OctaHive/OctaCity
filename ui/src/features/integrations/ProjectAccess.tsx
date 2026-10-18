import { useState } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import type { ProjectHierarchyApi, ProjectSummary } from '../../shared/projectHierarchy';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import {
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import {
  safeIntegrationError,
  type Connection,
  type IntegrationsApi,
  type ProjectAccess,
  type ProjectAccessIntent,
} from './api';
import styles from './Integrations.module.css';
import { sameReference } from './references';
import { ConnectionDependencies } from './ConnectionDependencies';

/** Select an existing visible OctaCity Project, independently of native provider resources. */
export function ProjectAccessPanel({
  api,
  connection,
  projects,
}: {
  api: IntegrationsApi;
  connection: Connection;
  projects: Pick<ProjectHierarchyApi, 'listProjects'>;
}) {
  const { t } = usePresentation();
  const [parent, setParent] = useState<ProjectSummary | null>(null);
  const [selected, setSelected] = useState<ProjectSummary | null>(null);
  const choices = useInfiniteQuery({
    queryKey: queryKeys.projectChildren(parent?.id ?? null),
    queryFn: ({ pageParam, signal }) =>
      projects.listProjects(parent?.id ?? null, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const items = choices.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <section className={styles.card} aria-labelledby="project-access-heading">
      <h2 id="project-access-heading">{t('integrations.projectAccess')}</h2>
      <p>{t('integrations.accessHelp')}</p>
      {parent ? (
        <p>
          {parent.name}{' '}
          <button type="button" onClick={() => setParent(null)}>
            {t('integrations.rootProjects')}
          </button>
        </p>
      ) : null}
      {choices.isPending ? <QueryLoadingNotice label={t('integrations.project')} /> : null}
      {choices.error ? (
        <QueryFailureNotice
          title={t('integrations.project')}
          error={safeIntegrationError(choices.error, t('integrations.safeFailure'))}
          onRetry={() => choices.refetch()}
        />
      ) : null}
      <SelectMenu
        ariaLabel={t('integrations.project')}
        value={items.some((p) => p.id === selected?.id) ? selected!.id : ''}
        options={[
          { value: '', label: t('integrations.chooseProject') },
          ...items.map((p) => ({ value: p.id, label: p.name })),
        ]}
        onValueChange={(id) => setSelected(items.find((p) => p.id === id) ?? null)}
      />
      {choices.hasNextPage ? (
        <button
          type="button"
          disabled={choices.isFetchingNextPage}
          onClick={() => void choices.fetchNextPage()}
        >
          {t('integrations.moreProjects')}
        </button>
      ) : null}
      {selected?.has_children ? (
        <button type="button" onClick={() => setParent(selected)}>
          {t('integrations.childProjects')}
        </button>
      ) : null}
      {selected ? (
        <SelectedAccess
          key={`${selected.id}:${connection.version}`}
          api={api}
          connection={connection}
          project={selected}
        />
      ) : null}
    </section>
  );
}
function SelectedAccess({
  api,
  connection,
  project,
}: {
  api: IntegrationsApi;
  connection: Connection;
  project: ProjectSummary;
}) {
  const { t } = usePresentation();
  const [editing, setEditing] = useState(false);
  const access = useQuery({
    queryKey: queryKeys.integrationProjectAccess(connection.id, project.id),
    queryFn: ({ signal }) => api.getProjectAccess(connection.id, project.id, signal),
  });
  const resources = useQuery({
    queryKey: queryKeys.integrationProjectResources(connection.id, project.id),
    queryFn: ({ signal }) => api.listProjectResources(connection.id, project.id, signal),
  });
  return (
    <div>
      <h3>{project.name}</h3>
      {access.isPending ? <QueryLoadingNotice label={t('integrations.projectAccess')} /> : null}
      {access.error ? (
        <QueryFailureNotice
          title={t('integrations.projectAccess')}
          error={safeIntegrationError(access.error, t('integrations.safeFailure'))}
          onRetry={() => access.refetch()}
        />
      ) : null}
      <h4>{t('integrations.usableResources')}</h4>
      {resources.isPending ? (
        <QueryLoadingNotice label={t('integrations.usableResources')} />
      ) : null}
      {resources.error ? (
        <QueryFailureNotice
          title={t('integrations.usableResources')}
          error={safeIntegrationError(resources.error, t('integrations.safeFailure'))}
          onRetry={() => resources.refetch()}
        />
      ) : null}
      {resources.data && !resources.error ? (
        resources.data.resources.length ? (
          <ul>
            {resources.data.resources.map((r) => (
              <li key={r.native_id}>
                {r.locator}:{' '}
                {r.capabilities.map((c) => t(`integrations.purpose.${c.purpose}`)).join(', ')}
              </li>
            ))}
          </ul>
        ) : (
          <QueryEmptyNotice>{t('integrations.noGrantedResources')}</QueryEmptyNotice>
        )
      ) : null}
      <ConnectionDependencies api={api} id={connection.id} project={project.id} />
      {access.data && !access.error && !editing ? (
        <button type="button" onClick={() => setEditing(true)}>
          {t('integrations.editAccess')}
        </button>
      ) : null}
      {editing && access.data ? (
        <AccessEditor
          api={api}
          connection={connection}
          project={project}
          access={access.data.access}
          onClose={() => setEditing(false)}
        />
      ) : null}
    </div>
  );
}
/** An explicit replacement grant; empty resources revoke all purposes, without changing Work. */
function AccessEditor({
  api,
  connection,
  project,
  access,
  onClose,
}: {
  api: IntegrationsApi;
  connection: Connection;
  project: ProjectSummary;
  access: ProjectAccess | null;
  onClose: () => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [version, setVersion] = useState(access?.version ?? null);
  const [resources, setResources] = useState<ProjectAccessIntent['resources']>(() =>
    access?.connection_version === connection.version ? structuredClone(access.resources) : [],
  );
  const [refreshRequired, setRefreshRequired] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{
    intent: ProjectAccessIntent;
    returnFocus: HTMLElement;
  } | null>(null);
  async function refresh() {
    setRefreshRequired(true);
    setError(null);
    try {
      const current = await api.getProjectAccess(connection.id, project.id);
      setVersion(current.access?.version ?? null);
      setRefreshRequired(false);
    } catch (failure) {
      setError(safeIntegrationError(failure, t('integrations.safeFailure')));
    }
  }
  async function execute(attempt: ConfirmedMutationAttempt<ProjectAccessIntent>) {
    try {
      const result = await api.publishProjectAccess(
        structuredClone(attempt.request) as ProjectAccessIntent,
        attempt.headers,
      );
      setVersion(result.access.version);
      return result;
    } catch (failure) {
      throw safeIntegrationError(failure, t('integrations.safeFailure'));
    }
  }
  return (
    <section aria-labelledby="access-editor-heading">
      <h4 id="access-editor-heading">{t('integrations.editAccess')}</h4>
      <p>
        {t('integrations.accessVersion', { version: version ?? 0, connection: connection.version })}
      </p>
      {access && access.connection_version !== connection.version ? (
        <p>{t('integrations.accessRepin')}</p>
      ) : null}
      {connection.definition.resources.map((resource) => (
        <fieldset key={resource.native_id}>
          <legend>{resource.locator}</legend>
          {resource.capabilities.map((capability) => (
            <label key={capability.purpose}>
              <input
                type="checkbox"
                checked={resources.some(
                  (r) =>
                    sameReference(r.reference, resource.reference) &&
                    r.purposes.includes(capability.purpose),
                )}
                disabled={review !== null}
                onChange={(event) => {
                  const same = (r: ProjectAccessIntent['resources'][number]) =>
                    sameReference(r.reference, resource.reference);
                  const purposes =
                    resources.find(same)?.purposes.filter((p) => p !== capability.purpose) ?? [];
                  if (event.target.checked) purposes.push(capability.purpose);
                  setResources([
                    ...resources.filter((r) => !same(r)),
                    ...(purposes.length ? [{ reference: resource.reference, purposes }] : []),
                  ]);
                }}
              />
              {t(`integrations.purpose.${capability.purpose}`)}
            </label>
          ))}
        </fieldset>
      ))}
      {error ? (
        <QueryFailureNotice
          title={t('integrations.projectAccess')}
          error={error}
          onRetry={refresh}
        />
      ) : null}
      <button
        type="button"
        disabled={refreshRequired || review !== null}
        onClick={(event) =>
          setReview({
            intent: {
              connection_id: connection.id,
              connection_version: connection.version,
              project_id: project.id,
              expected_current_version: version,
              resources: structuredClone(resources),
            },
            returnFocus: event.currentTarget,
          })
        }
      >
        {t('integrations.reviewAccess')}
      </button>
      <button type="button" disabled={review !== null} onClick={onClose}>
        {t('common.close')}
      </button>
      {review ? (
        <ConfirmedCommand
          title={t('integrations.confirmAccess')}
          confirmLabel={t('integrations.confirmChange')}
          consequence={t('integrations.accessConsequence', {
            project: project.name,
            connection: connection.definition.name,
            version: connection.version,
            purposes: review.intent.resources.length
              ? review.intent.resources
                  .map(
                    (r) =>
                      `${connection.definition.resources.find((item) => sameReference(item.reference, r.reference))?.locator}: ${r.purposes.map((p) => t(`integrations.purpose.${p}`)).join(', ')}`,
                  )
                  .join('; ')
              : t('integrations.revokeAll'),
          })}
          request={review.intent}
          execute={execute}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.integrationProjectAccess(connection.id, project.id) },
            { queryKey: queryKeys.integrationProjectResources(connection.id, project.id) },
            { queryKey: queryKeys.factoryChoices(project.id) },
            { queryKey: queryKeys.audit },
          ]}
          {...(review.intent.expected_current_version === null
            ? {}
            : { version: review.intent.expected_current_version })}
          returnFocus={review.returnFocus}
          onRefresh={refresh}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('integrations.accessPublished')}</p>}
        />
      ) : null}
    </section>
  );
}
