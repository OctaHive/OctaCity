import { useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import type { IssueWorkflowsApi, WorkflowResource } from '../../api/issueWorkflows';
import type { Issue, IssuePolicy, IssuesApi } from './api';
import styles from './Issues.module.css';

/** Adoption is separate from editing requirements or changing the default for new tasks. */
export function IssueAdoption({
  api,
  workflows,
  issue,
  workflow,
  policy,
  ready,
  refresh,
}: {
  api: IssuesApi;
  workflows: IssueWorkflowsApi;
  issue: Issue;
  workflow: WorkflowResource | undefined;
  policy: IssuePolicy;
  ready: boolean;
  refresh: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const toggleRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [after, setAfter] = useState<string | null>(null);
  const [selected, setSelected] = useState<{ id: string; version: number } | null>(null);
  const [requested, setRequested] = useState<{ id: string; version: number } | null>(null);
  const [mapping, setMapping] = useState<Record<string, string>>({});
  const [review, setReview] = useState<{
    request: components['schemas']['AdoptIssueWorkflowRequest'];
    version: number;
    summary: string;
    focus: HTMLElement;
  } | null>(null);
  const catalog = useQuery({
    queryKey: queryKeys.issueWorkflowPage(issue.project_id, after),
    enabled: open,
    queryFn: ({ signal }) => workflows.list(issue.project_id, after, signal),
  });
  const target = useQuery({
    queryKey: queryKeys.issueWorkflow(issue.project_id, requested?.id ?? null, requested?.version),
    enabled: requested !== null,
    queryFn: ({ signal }) =>
      workflows.get(issue.project_id, requested!.id, requested!.version, signal),
  });
  const resource = target.data;
  const currentStatus = workflow?.definition.statuses.find((s) => s.id === issue.status_id);
  const targetStatus = resource?.definition.statuses.find((s) => s.id === mapping[issue.status_id]);
  const ownsStatus = policy.source_owned.includes('status');
  const statusMapping = policy.status_mapping_workflow;
  const compatible =
    !statusMapping ||
    (statusMapping.id === resource?.id && statusMapping.version === resource.version);
  const canReview =
    ready &&
    !!currentStatus &&
    !!targetStatus &&
    !ownsStatus &&
    compatible &&
    target.error === null &&
    !target.isFetching;
  return (
    <section className={styles.panel} aria-label={t('issues.adoption')}>
      <h2>{t('issues.adoption')}</h2>
      <p>{t('issues.adoptionHelp')}</p>
      <button ref={toggleRef} onClick={() => setOpen(!open)}>
        {t(open ? 'command.abandon' : 'issues.changeWorkflow')}
      </button>
      {open ? (
        <>
          {catalog.isPending ? (
            <QueryLoadingNotice label={t('issues.targetWorkflow')} />
          ) : catalog.data === undefined ? (
            <QueryFailureNotice
              title={t('issues.targetWorkflow')}
              error={catalog.error}
              onRetry={catalog.refetch}
            />
          ) : (
            <>
              <QueryBackgroundNotice
                label={t('issues.targetWorkflow')}
                error={catalog.error}
                fetching={catalog.isFetching}
                onRetry={catalog.refetch}
              />
              {catalog.data.items.length === 0 ? (
                <QueryEmptyNotice>{t('issues.noWorkflows')}</QueryEmptyNotice>
              ) : (
                <div>
                  {t('issues.targetWorkflow')}
                  <SelectMenu
                    ariaLabel={t('issues.targetWorkflow')}
                    value={selected?.id ?? ''}
                    options={[
                      { value: '', label: t('issues.chooseWorkflow') },
                      ...catalog.data.items.map((item) => ({ value: item.id, label: item.name })),
                    ]}
                    onValueChange={(value) => {
                      const item = catalog.data.items.find((w) => w.id === value);
                      setSelected(item ? { id: item.id, version: item.version } : null);
                      setRequested(null);
                      setMapping({});
                    }}
                  />
                </div>
              )}
              {after ? (
                <button
                  onClick={() => {
                    setAfter(null);
                    setSelected(null);
                    setRequested(null);
                  }}
                >
                  {t('issues.firstPage')}
                </button>
              ) : null}
              {catalog.data.next_cursor ? (
                <button
                  onClick={() => {
                    setAfter(catalog.data!.next_cursor);
                    setSelected(null);
                    setRequested(null);
                  }}
                >
                  {t('issues.nextPage')}
                </button>
              ) : null}
            </>
          )}
          {selected ? (
            <>
              <label>
                {t('workflow.versionInput')}
                <input
                  type="number"
                  min={1}
                  step={1}
                  value={selected.version}
                  onChange={(e) => {
                    setSelected({ ...selected, version: e.target.valueAsNumber });
                    setRequested(null);
                    setMapping({});
                  }}
                />
              </label>
              <button
                disabled={
                  !Number.isSafeInteger(selected.version) ||
                  selected.version < 1 ||
                  !!catalog.error ||
                  catalog.isFetching
                }
                onClick={() => {
                  setRequested({ ...selected });
                  setMapping({});
                }}
              >
                {t('issues.loadWorkflow')}
              </button>
            </>
          ) : null}
          {requested ? (
            target.isPending ? (
              <QueryLoadingNotice label={t('issues.targetWorkflow')} />
            ) : resource === undefined ? (
              <QueryFailureNotice
                title={t('issues.targetWorkflow')}
                error={target.error}
                onRetry={target.refetch}
              />
            ) : (
              <>
                <QueryBackgroundNotice
                  label={t('issues.targetWorkflow')}
                  fetching={target.isFetching}
                  error={target.error}
                  onRetry={target.refetch}
                />
                <p>
                  {resource.definition.name} · {t('issues.revision', { version: resource.version })}
                </p>
                <div>
                  {t('issues.correspondence', {
                    status: currentStatus?.name ?? t('issues.unknownStatus'),
                  })}
                  <SelectMenu
                    ariaLabel={t('issues.correspondence', {
                      status: currentStatus?.name ?? t('issues.unknownStatus'),
                    })}
                    value={mapping[issue.status_id] ?? ''}
                    options={[
                      { value: '', label: t('issues.chooseStatus') },
                      ...resource.definition.statuses.map((item) => ({
                        value: item.id,
                        label: item.name,
                      })),
                    ]}
                    onValueChange={(value) => setMapping({ [issue.status_id]: value })}
                  />
                </div>
                {ownsStatus ? (
                  <p>{t('issues.block.source_owned')}</p>
                ) : !compatible ? (
                  <p>{t('issues.mappingConflict')}</p>
                ) : null}
                <button
                  disabled={!canReview}
                  onClick={(event) =>
                    setReview({
                      request: {
                        expected_workflow_id: issue.workflow_id,
                        expected_workflow_version: issue.workflow_version,
                        expected_status_id: issue.status_id,
                        target_workflow_id: resource.id,
                        target_workflow_version: resource.version,
                        status_correspondence: { ...mapping },
                        expected_field_policy: structuredClone(policy),
                      },
                      version: issue.version,
                      summary: `${resource.definition.name} · ${t('issues.revision', { version: resource.version })} · ${currentStatus!.name} → ${targetStatus!.name}`,
                      focus: event.currentTarget,
                    })
                  }
                >
                  {t('issues.reviewAdoption')}
                </button>
              </>
            )
          ) : null}
        </>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('issues.reviewAdoption')}
          confirmLabel={t('issues.applyAdoption')}
          consequence={`${review.summary}. ${t('issues.adoptionHelp')}`}
          request={review.request}
          version={review.version}
          returnFocus={canReview ? review.focus : (toggleRef.current ?? review.focus)}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issues(issue.project_id) }]}
          execute={(attempt) =>
            api.adopt(
              issue.project_id,
              issue.id,
              structuredClone(
                attempt.request,
              ) as components['schemas']['AdoptIssueWorkflowRequest'],
              requireVersionedMutationHeaders(attempt.headers),
            )
          }
          onRefresh={async () => {
            setMapping({});
            await refresh();
          }}
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <p>{t('issues.adoptionSaved', { version: result.resource.version })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
