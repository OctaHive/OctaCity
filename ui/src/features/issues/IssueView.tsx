import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryFailureNotice,
  QueryLoadingNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import type { PipelineTargetSources } from '../../shared/PipelineTargetFields';
import type { IssueExecutionsApi } from './executionApi';
import { IssueExecutions } from './IssueExecutions';
import { IssueExecutionForm } from './IssueExecutionForm';
import type { IssuesApi } from './api';
import { IssueEditor } from './IssueEditor';
import { IssueRelationships } from './IssueRelationships';
import { IssueProperties } from './IssueProperties';
import { IssueProjectNav, type IssueProjectReader } from './IssueProjectNav';
import { IssueRevisions } from './IssueRevisions';
import { IssueAdoption } from './IssueAdoption';
import { IssueMoves } from './IssueMoves';
import { IssueHistory } from './IssueHistory';
import styles from './Issues.module.css';
const sections = ['overview', 'activity', 'runs', 'results', 'actions'] as const;
export function IssueView({
  api,
  workflows,
  projects,
  executions,
  executionSources,
}: {
  api: IssuesApi;
  workflows: IssueWorkflowsApi;
  projects: IssueProjectReader;
  executions: IssueExecutionsApi;
  executionSources: PipelineTargetSources;
}) {
  const { projectId, issueId } = useParams<'projectId' | 'issueId'>();
  return projectId && issueId ? (
    <Card
      key={projectId + issueId}
      executions={executions}
      executionSources={executionSources}
      api={api}
      workflows={workflows}
      projects={projects}
      projectId={projectId}
      issueId={issueId}
    />
  ) : null;
}
function Card({
  api,
  workflows,
  projects,
  executions,
  executionSources,
  projectId,
  issueId,
}: {
  api: IssuesApi;
  workflows: IssueWorkflowsApi;
  projects: IssueProjectReader;
  executions: IssueExecutionsApi;
  executionSources: PipelineTargetSources;
  projectId: string;
  issueId: string;
}) {
  const { t, locale } = usePresentation();
  const queryClient = useQueryClient();
  const [params, setParams] = useSearchParams();
  const section = sections.find((s) => s === params.get('section')) ?? 'overview';
  const [editing, setEditing] = useState(false);
  const [starting, setStarting] = useState<'requirements' | 'inputs' | null>(null);
  const selectedRevision = Number(params.get('revision'));
  const hasRevision = Number.isSafeInteger(selectedRevision) && selectedRevision > 0;
  function sectionSearch(value: string) {
    const next = new URLSearchParams();
    if (value !== 'overview') next.set('section', value);
    if (params.get('execution')) next.set('execution', params.get('execution')!);
    if (hasRevision) next.set('revision', String(selectedRevision));
    return next.toString();
  }
  const [propertiesOpen, setPropertiesOpen] = useState(false);
  const propertiesRef = useRef<HTMLElement>(null);
  const propertiesTriggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (propertiesOpen) propertiesRef.current?.focus();
  }, [propertiesOpen]);
  const current = useQuery({
    queryKey: queryKeys.issue(projectId, issueId),
    gcTime: 0,
    queryFn: ({ signal }) => api.get(projectId, issueId, signal),
  });
  const issue = current.data;
  const workflow = useQuery({
    queryKey: queryKeys.issueWorkflow(
      projectId,
      issue?.workflow_id ?? null,
      issue?.workflow_version,
    ),
    enabled: issue !== undefined,
    queryFn: ({ signal }) =>
      workflows.get(projectId, issue!.workflow_id, issue!.workflow_version, signal),
  });
  const choices = useQuery({
    queryKey: queryKeys.issueTransitions(projectId, issueId),
    gcTime: 0,
    enabled: issue !== undefined,
    queryFn: ({ signal }) => api.transitions(projectId, issueId, signal),
  });
  const fresh =
    !!issue &&
    current.error === null &&
    !current.isFetching &&
    !choices.isFetching &&
    choices.error === null &&
    choices.data?.issue_version === issue.version &&
    choices.data.workflow_id === issue.workflow_id &&
    choices.data.workflow_version === issue.workflow_version &&
    choices.data.status_id === issue.status_id;
  async function refresh() {
    await current.refetch();
    await Promise.all([
      choices.refetch(),
      queryClient.invalidateQueries({ queryKey: queryKeys.issueChildrenRoot(projectId, issueId) }),
      ...(issue?.planning?.parent_id
        ? [
            queryClient.invalidateQueries({
              queryKey: queryKeys.issue(projectId, issue.planning.parent_id),
            }),
          ]
        : []),
      queryClient.invalidateQueries({ queryKey: queryKeys.issuePropertyChoices(projectId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.issueHistoryRoot(projectId, issueId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.issueRevisionsRoot(projectId, issueId) }),
    ]);
  }
  return (
    <section className={styles.page}>
      <IssueProjectNav projects={projects} projectId={projectId} card />
      {current.isPending ? (
        <QueryLoadingNotice label={t('issues.task')} />
      ) : issue === undefined ? (
        <QueryFailureNotice
          title={t('issues.task')}
          error={current.error}
          onRetry={current.refetch}
        />
      ) : (
        <>
          <header className={`${styles.panel} ${styles.header}`}>
            <div className={styles.headerTitle}>
              <h1>
                {issue.display_key} · {issue.content.title}
              </h1>
              <p>
                {t('issues.businessStatus', {
                  status:
                    workflow.data?.definition.statuses.find((s) => s.id === issue.status_id)
                      ?.name ?? t('issues.unknownStatus'),
                })}
              </p>
              <p>{t('issues.statusHelp')}</p>
            </div>
            <div className={styles.headerActions}>
              <button
                type="button"
                disabled={starting !== null || current.isFetching || current.error !== null}
                onClick={() =>
                  setStarting(issue.content.acceptance?.trim() ? 'inputs' : 'requirements')
                }
                aria-describedby="issue-start-help"
              >
                {t('issues.start')}
              </button>
              <button
                disabled={current.isFetching || choices.isFetching}
                onClick={() => void refresh()}
              >
                {t('issues.refresh')}
              </button>
              <p id="issue-start-help">{t('issues.startHelp')}</p>
            </div>
          </header>
          {starting === 'inputs' ? (
            <IssueExecutionForm
              issue={issue}
              api={executions}
              sources={executionSources}
              ready={current.error === null && !current.isFetching}
              onRefresh={refresh}
              onSelect={(id) =>
                setParams((currentParams) => {
                  const next = new URLSearchParams(currentParams);
                  next.set('execution', id);
                  return next;
                })
              }
              onClose={() => setStarting(null)}
            />
          ) : null}
          {starting === 'requirements' ? (
            <section className={styles.panel} role="status">
              <p>{t('issues.executionAcceptanceRequired')}</p>
              <button
                type="button"
                onClick={() => {
                  setParams(sectionSearch('overview'));
                  setEditing(true);
                  setStarting(null);
                }}
              >
                {t('issues.addAcceptance')}
              </button>
            </section>
          ) : null}
          {hasRevision ? (
            <Link to={{ search: sectionSearch('activity') }}>
              {t('issues.historicalSelection', { version: selectedRevision })}
            </Link>
          ) : null}
          <QueryBackgroundNotice
            label={t('issues.task')}
            fetching={current.isFetching}
            error={current.error}
            onRetry={current.refetch}
          />
          {workflow.error ? (
            <QueryFailureNotice
              title={t('issues.workflow')}
              error={workflow.error}
              onRetry={workflow.refetch}
            />
          ) : null}
          {section !== 'actions' && choices.error ? (
            <QueryFailureNotice title={t('issues.moves')} error={choices.error} onRetry={refresh} />
          ) : null}
          <nav className={styles.actions} aria-label={t('issues.sections')}>
            {sections.map((s) => (
              <Link
                key={s}
                aria-current={section === s ? 'page' : undefined}
                to={{ search: sectionSearch(s) }}
              >
                {t(`issues.${s}`)}
              </Link>
            ))}
          </nav>
          <button
            className={styles.propertiesTrigger}
            ref={propertiesTriggerRef}
            aria-expanded={propertiesOpen}
            aria-controls="issue-properties"
            onClick={() => setPropertiesOpen(!propertiesOpen)}
          >
            {t('issues.properties')}
          </button>
          <div className={styles.columns}>
            <div className={styles.main}>
              {section === 'overview' ? (
                <>
                  <section className={styles.panel}>
                    <button disabled={!fresh} onClick={() => setEditing(!editing)}>
                      {t(editing ? 'command.abandon' : 'issues.edit')}
                    </button>
                    {editing ? (
                      <IssueEditor
                        api={api}
                        projectId={projectId}
                        issue={issue}
                        policy={choices.data!.field_policy}
                        ready={fresh}
                        onRefresh={refresh}
                      />
                    ) : null}
                    <h2>{t('issues.description')}</h2>
                    <p className={styles.prose}>
                      {issue.content.description || t('issues.noDescription')}
                    </p>
                    <h2>{t('issues.acceptance')}</h2>
                    <p className={styles.prose}>
                      {issue.content.acceptance || t('issues.noAcceptance')}
                    </p>
                    <p>{t('issues.revision', { version: issue.version })}</p>
                  </section>
                  <IssueHistory api={api} workflows={workflows} issue={issue} preview />
                </>
              ) : section === 'activity' ? (
                <>
                  <IssueHistory api={api} workflows={workflows} issue={issue} />
                  <IssueRevisions api={api} issue={issue} />
                </>
              ) : section === 'runs' ? (
                <IssueExecutions api={executions} issue={issue} />
              ) : section === 'actions' ? (
                <>
                  {choices.isPending ? (
                    <QueryLoadingNotice label={t('issues.moves')} />
                  ) : !choices.data ? (
                    <QueryFailureNotice
                      title={t('issues.moves')}
                      error={choices.error}
                      onRetry={refresh}
                    />
                  ) : (
                    <>
                      <QueryBackgroundNotice
                        label={t('issues.moves')}
                        fetching={choices.isFetching}
                        error={choices.error}
                        onRetry={refresh}
                      />
                      <IssueMoves
                        api={api}
                        issue={issue}
                        choices={choices.data}
                        workflow={workflow.data}
                        ready={fresh && workflow.error === null && !workflow.isFetching}
                        refresh={refresh}
                      />
                      <IssueAdoption
                        api={api}
                        workflows={workflows}
                        issue={issue}
                        workflow={workflow.data}
                        policy={choices.data.field_policy}
                        ready={fresh && workflow.error === null && !workflow.isFetching}
                        refresh={refresh}
                      />
                    </>
                  )}
                </>
              ) : (
                <section className={styles.panel}>
                  <h2>{t(`issues.${section}`)}</h2>
                  <p>{t('issues.executionUnavailable')}</p>
                </section>
              )}
            </div>
            <aside
              id="issue-properties"
              ref={propertiesRef}
              tabIndex={-1}
              className={`${styles.properties} ${propertiesOpen ? styles.propertiesOpen : ''}`}
              aria-label={t('issues.properties')}
            >
              <button
                className={styles.propertiesTrigger}
                onClick={() => {
                  setPropertiesOpen(false);
                  propertiesTriggerRef.current?.focus();
                }}
              >
                {t('issues.closeProperties')}
              </button>
              <section className={styles.panel}>
                <h2>{t('issues.properties')}</h2>
                <dl>
                  <dt>{t('issues.priority')}</dt>
                  <dd>{issue.content.priority ?? t('issues.notSet')}</dd>
                  <dt>{t('issues.workflow')}</dt>
                  <dd>
                    {workflow.data?.definition.name ?? t('issues.unknownStatus')} ·{' '}
                    {t('issues.revision', { version: issue.workflow_version })}
                  </dd>
                </dl>
                <IssueRelationships api={api} issue={issue} />
                <IssueProperties
                  api={api}
                  issue={issue}
                  policy={choices.data?.field_policy}
                  ready={fresh}
                  onRefresh={refresh}
                />
              </section>
              <section className={styles.panel}>
                <h2>{t('issues.execution')}</h2>
                <Link to={{ search: sectionSearch('runs') }}>{t('issues.executionHistory')}</Link>
              </section>
              <details className={styles.panel}>
                <summary>{t('issues.details')}</summary>
                <dl>
                  <dt>{t('issues.identity')}</dt>
                  <dd>{issue.id}</dd>
                  <dt>{t('issues.created')}</dt>
                  <dd>{new Date(issue.created_at_unix_ms).toLocaleString(locale)}</dd>
                  <dt>{t('issues.updated')}</dt>
                  <dd>{new Date(issue.updated_at_unix_ms).toLocaleString(locale)}</dd>
                </dl>
              </details>
            </aside>
          </div>
        </>
      )}
    </section>
  );
}
