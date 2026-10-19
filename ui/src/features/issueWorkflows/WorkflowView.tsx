import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { projectPath } from '../../app/routes';
import { queryKeys } from '../../app/query';
import {
  QueryFailureNotice,
  QueryLoadingNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import { WorkflowEditor } from './WorkflowEditor';
import { WorkflowCatalog } from './WorkflowCatalog';
import type { IssueWorkflowsApi, WorkflowResource } from '../../api/issueWorkflows';
import styles from './WorkflowView.module.css';

export function WorkflowView({ api }: { api: IssueWorkflowsApi }) {
  const { projectId } = useParams<'projectId'>();
  return projectId ? <ProjectWorkflow key={projectId} api={api} projectId={projectId} /> : null;
}
function ProjectWorkflow({ api, projectId }: { api: IssueWorkflowsApi; projectId: string }) {
  const { t } = usePresentation();
  const current = useQuery({
    queryKey: queryKeys.issueWorkflowDefault(projectId),
    queryFn: ({ signal }) => api.getDefault(projectId, signal),
  });
  const [editing, setEditing] = useState<{ resource: WorkflowResource; creating: boolean } | null>(
    null,
  );
  return (
    <section className={styles.page} aria-labelledby="workflow-title">
      <Link to={projectPath(projectId)}>{t('workflow.back')}</Link>
      <h1 id="workflow-title">{t('workflow.title')}</h1>
      <p>{t('workflow.help')}</p>
      {current.isPending ? (
        <QueryLoadingNotice label={t('workflow.title')} />
      ) : current.data === undefined ? (
        <QueryFailureNotice
          title={t('workflow.loadFailure')}
          error={current.error}
          onRetry={current.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            error={current.error}
            fetching={current.isFetching}
            label={t('workflow.title')}
            onRetry={current.refetch}
          />
          <section className={styles.card} aria-label={t('workflow.default')}>
            <h2>{t('workflow.default')}</h2>
            <p>
              {current.data.workflow.definition.name} ·{' '}
              {t('workflow.version', { version: current.data.workflow.version })}
            </p>
            <button
              type="button"
              onClick={() =>
                setEditing({ resource: structuredClone(current.data.workflow), creating: false })
              }
            >
              {t('workflow.editDefault')}
            </button>
            <button
              type="button"
              onClick={() =>
                setEditing({
                  resource: { ...structuredClone(current.data.workflow), id: crypto.randomUUID() },
                  creating: true,
                })
              }
            >
              {t('workflow.create')}
            </button>
          </section>
        </>
      )}
      <WorkflowCatalog
        api={api}
        projectId={projectId}
        selection={current.error === null && !current.isFetching ? current.data : undefined}
        refreshSelection={async () => {
          await current.refetch();
        }}
        onEdit={(resource) => setEditing({ resource, creating: false })}
      />
      {editing ? (
        <WorkflowEditor
          key={editing.resource.id + ':' + editing.resource.version}
          api={api}
          projectId={projectId}
          baseline={editing.resource}
          creating={editing.creating}
        />
      ) : null}
    </section>
  );
}
