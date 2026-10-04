import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, GitBranch, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';

import { ManagementApiError } from '../../api/client';
import { queryInvalidations, queryKeys } from '../../app/query';
import { projectPath } from '../../app/routes';
import { formatEnumLabel, formatTimestamp } from '../../shared/display';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { AttemptGraph, edgeLabel, policyLabel, stateFamily, stateLabel } from './AttemptGraph';
import type {
  AttemptResource,
  BuildCommandApi,
  BuildDiagnosticsApi,
  BuildResource,
  JobResource,
} from './api';
import { BuildLogSearch } from './BuildLogSearch';
import { DiagnosticFailure, DiagnosticLoading, DiagnosticStale } from './BuildDiagnosticState';
import { BuildResultDiagnostics } from './BuildResultDiagnostics';
import styles from './Builds.module.css';
import { JobEvents } from './JobEvents';
import type { JobEventTarget } from './jobEventFollower';

const JOB_SELECTION_PARAMETER = 'job_id';

export function BuildView({ api }: { api: BuildDiagnosticsApi }) {
  const { buildId } = useParams<'buildId'>();
  if (buildId === undefined) return <BuildNotFound />;
  return <SelectedBuild api={api} buildId={buildId} />;
}

function SelectedBuild({ api, buildId }: { api: BuildDiagnosticsApi; buildId: string }) {
  const build = useQuery({
    queryFn: ({ signal }) => api.getBuild(buildId, signal),
    queryKey: queryKeys.build(buildId),
  });

  if (build.isPending) return <BuildLoading />;
  if (build.data === undefined) {
    return isNotFound(build.error) ? (
      <BuildNotFound />
    ) : (
      <DiagnosticFailure error={build.error} label="Build" onRetry={build.refetch} />
    );
  }

  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <BuildHeading
        api={api}
        build={build.data}
        isFetching={build.isFetching}
        onRefresh={build.refetch}
      />
      {build.error === null ? null : <DiagnosticStale label="Build" onRetry={build.refetch} />}
      <AttemptPanel api={api} build={build.data} />
      <BuildResultDiagnostics api={api} buildId={build.data.id} />
    </section>
  );
}

function BuildHeading({
  api,
  build,
  isFetching,
  onRefresh,
}: {
  api: BuildCommandApi;
  build: BuildResource;
  isFetching: boolean;
  onRefresh: () => unknown;
}) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const queryClient = useQueryClient();
  const [command, setCommand] = useState<{
    kind: 'cancel' | 'retry';
    returnFocus: HTMLElement;
  } | null>(null);
  const created = formatTimestamp(build.created_at_unix_ms);
  const openCommand = (kind: 'cancel' | 'retry', returnFocus: HTMLElement) =>
    setCommand({ kind, returnFocus });
  return (
    <header className={styles.heading}>
      <Link className={styles.backLink} to={projectPath(build.project_id)}>
        <ArrowLeft aria-hidden="true" size={15} />
        Project
      </Link>
      <div className={styles.headingRow}>
        <div>
          <p className={styles.eyebrow}>Build diagnostics</p>
          <h1 id="page-title" ref={headingRef} tabIndex={-1}>
            Build {build.id}
          </h1>
        </div>
        <div className={styles.headingActions}>
          {build.state === 'queued' || build.state === 'running' ? (
            <button
              className={styles.secondaryButton}
              onClick={(event) => openCommand('cancel', event.currentTarget)}
              type="button"
            >
              Cancel Build
            </button>
          ) : null}
          {build.state === 'failed' ? (
            <button
              className={styles.secondaryButton}
              onClick={(event) => openCommand('retry', event.currentTarget)}
              type="button"
            >
              Retry Build
            </button>
          ) : null}
          <button
            className={styles.secondaryButton}
            disabled={isFetching}
            onClick={() => void onRefresh()}
            type="button"
          >
            <RefreshCw aria-hidden="true" size={15} />
            {isFetching ? 'Refreshing' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className={styles.buildSummary}>
        <Status state={build.state} />
        <span>Attempt {build.current_attempt.number}</span>
        <span>Priority {build.priority}</span>
        <time dateTime={created.machine ?? undefined}>{created.display}</time>
      </div>
      <dl className={styles.definitionGrid}>
        <Definition
          label="Configuration"
          value={`${build.configuration_id} v${build.configuration_version}`}
        />
        <Definition label="Pipeline" value={`${build.pipeline_id} v${build.pipeline_version}`} />
        <Definition
          label="Repository"
          value={`${build.repository_id} v${build.repository_version}`}
        />
        <Definition label="Revision" value={build.immutable_revision} />
      </dl>
      {command === null ? null : command.kind === 'cancel' ? (
        <ConfirmedCommand
          confirmLabel="Cancel Build"
          consequence={`This requests cancellation of active Build ${build.id} and its unfinished Jobs.`}
          execute={({ headers, request }) => api.cancelBuild(request.buildId, headers)}
          fallbackFocusRef={headingRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.build(build.id) },
            { queryKey: queryKeys.projectBuildPages(build.project_id) },
            queryInvalidations.audit,
          ]}
          onClose={() => setCommand(null)}
          queryClient={queryClient}
          renderSuccess={(result) => (
            <span>Cancellation accepted ({formatEnumLabel(result.disposition)}).</span>
          )}
          request={{ buildId: build.id }}
          returnFocus={command.returnFocus}
          title={`Cancel Build ${build.id}?`}
        />
      ) : (
        <ConfirmedCommand
          confirmLabel="Retry Build"
          consequence={`This creates the next Attempt from failed Build ${build.id}.`}
          execute={({ headers, request }) => api.retryBuild(request.buildId, headers)}
          fallbackFocusRef={headingRef}
          invalidations={[
            { exact: true, queryKey: queryKeys.build(build.id) },
            { queryKey: queryKeys.projectBuildPages(build.project_id) },
            queryInvalidations.audit,
          ]}
          onClose={() => setCommand(null)}
          queryClient={queryClient}
          renderSuccess={(result) => (
            <span>
              Retry Attempt {result.attempt_number} created ({formatEnumLabel(result.disposition)}).
            </span>
          )}
          request={{ buildId: build.id }}
          returnFocus={command.returnFocus}
          title={`Retry Build ${build.id}?`}
        />
      )}
    </header>
  );
}

function AttemptPanel({ api, build }: { api: BuildDiagnosticsApi; build: BuildResource }) {
  const attemptId = build.current_attempt.id;
  const attempt = useQuery({
    queryFn: ({ signal }) => api.getAttempt(attemptId, signal),
    queryKey: queryKeys.attempt(attemptId),
  });

  if (attempt.isPending) return <DiagnosticLoading label="current Attempt" />;
  if (attempt.data === undefined) {
    return (
      <DiagnosticFailure error={attempt.error} label="Current Attempt" onRetry={attempt.refetch} />
    );
  }
  return (
    <AttemptDiagnostics
      api={api}
      attempt={attempt.data}
      build={build}
      isFetching={attempt.isFetching}
      onRefresh={attempt.refetch}
      stale={attempt.error !== null}
    />
  );
}

function AttemptDiagnostics({
  api,
  attempt,
  build,
  isFetching,
  onRefresh,
  stale,
}: {
  api: BuildDiagnosticsApi;
  attempt: AttemptResource;
  build: BuildResource;
  isFetching: boolean;
  onRefresh: () => unknown;
  stale: boolean;
}) {
  const [searchParameters, setSearchParameters] = useSearchParams();
  const queryClient = useQueryClient();
  const requestedJobId = searchParameters.get(JOB_SELECTION_PARAMETER);
  const selectedJob =
    attempt.jobs.find((job) => job.id === requestedJobId) ?? attempt.jobs[0] ?? null;
  const selectedJobId = selectedJob?.id ?? null;

  useEffect(() => {
    if (requestedJobId === null || selectedJob?.id === requestedJobId) return;
    const next = new URLSearchParams(searchParameters);
    next.delete(JOB_SELECTION_PARAMETER);
    setSearchParameters(next, { replace: true });
  }, [requestedJobId, searchParameters, selectedJob?.id, setSearchParameters]);

  function selectJob(jobId: string) {
    const next = new URLSearchParams(searchParameters);
    next.set(JOB_SELECTION_PARAMETER, jobId);
    setSearchParameters(next);
  }

  const updateObservedJob = useCallback(
    (observed: JobEventTarget) => {
      queryClient.setQueryData<AttemptResource>(queryKeys.attempt(attempt.attempt.id), (current) =>
        current === undefined
          ? current
          : {
              ...current,
              jobs: current.jobs.map((job) =>
                job.id === observed.id ? { ...job, ...observed } : job,
              ),
            },
      );
    },
    [attempt.attempt.id, queryClient],
  );

  return (
    <div className={styles.attemptStack}>
      <section aria-labelledby="attempt-heading" className={styles.panel}>
        <div className={styles.panelHeading}>
          <div>
            <p className={styles.eyebrow}>Current Attempt</p>
            <h2 id="attempt-heading">Attempt {attempt.attempt.number}</h2>
            <p className={styles.monospace}>{attempt.attempt.id}</p>
          </div>
          <div className={styles.panelActions}>
            <Status state={attempt.attempt.state} />
            <button
              className={styles.textButton}
              disabled={isFetching}
              onClick={() => void onRefresh()}
              type="button"
            >
              <RefreshCw aria-hidden="true" size={14} />
              {isFetching ? 'Refreshing' : 'Refresh Attempt'}
            </button>
          </div>
        </div>
        {stale ? <DiagnosticStale label="Attempt" onRetry={onRefresh} /> : null}
        {attempt.jobs.length === 0 ? (
          <div className={styles.emptyState}>
            <GitBranch aria-hidden="true" size={26} />
            <p>This Attempt has no Jobs.</p>
          </div>
        ) : (
          <>
            <nav aria-label="Jobs" className={styles.jobSelector}>
              {attempt.jobs.map((job) => (
                <button
                  aria-label={`Select ${job.pipeline_node_id}, ${stateLabel(job.state)}`}
                  aria-pressed={job.id === selectedJobId}
                  className={styles.jobButton}
                  key={job.id}
                  onClick={() => selectJob(job.id)}
                  type="button"
                >
                  <span>{job.pipeline_node_id}</span>
                  <Status state={job.state} />
                </button>
              ))}
            </nav>
            <div className={styles.graphPanel}>
              <AttemptGraph edges={attempt.edges} jobs={attempt.jobs} />
            </div>
            <DependencyTable attempt={attempt} onSelect={selectJob} selectedJobId={selectedJobId} />
          </>
        )}
      </section>
      {selectedJob === null ? null : (
        <JobDetails api={api} job={selectedJob} onJobUpdate={updateObservedJob} />
      )}
      <BuildLogSearch
        api={api}
        attemptId={attempt.attempt.id}
        buildId={build.id}
        jobs={attempt.jobs}
        projectId={build.project_id}
      />
    </div>
  );
}

function DependencyTable({
  attempt,
  onSelect,
  selectedJobId,
}: {
  attempt: AttemptResource;
  onSelect: (jobId: string) => void;
  selectedJobId: string | null;
}) {
  const jobsById = new Map(attempt.jobs.map((job) => [job.id, job]));
  return (
    <div className={styles.tableScroller}>
      <table aria-label="Attempt dependency table" className={styles.dependencyTable}>
        <thead>
          <tr>
            <th scope="col">Job</th>
            <th scope="col">State</th>
            <th scope="col">Depends on</th>
            <th scope="col">Policy</th>
          </tr>
        </thead>
        <tbody>
          {attempt.jobs.map((job) => {
            const incomingEdges = attempt.edges.filter((edge) => edge.dependent_job_id === job.id);
            return (
              <tr
                className={job.id === selectedJobId ? styles.selectedRow : undefined}
                key={job.id}
              >
                <th scope="row">
                  <button
                    aria-label={`Select ${job.pipeline_node_id}`}
                    className={styles.jobLink}
                    onClick={() => onSelect(job.id)}
                    type="button"
                  >
                    <span className={styles.visuallyHidden}>Select </span>
                    {job.pipeline_node_id}
                  </button>
                </th>
                <td>
                  <Status state={job.state} />
                </td>
                <td>
                  {incomingEdges.length === 0 ? (
                    'None'
                  ) : (
                    <ul className={styles.dependencies}>
                      {incomingEdges.map((edge) => (
                        <li
                          aria-label={edgeLabel(edge, attempt.jobs)}
                          key={edge.predecessor_job_id}
                        >
                          {jobsById.get(edge.predecessor_job_id)?.pipeline_node_id ??
                            edge.predecessor_job_id}
                        </li>
                      ))}
                    </ul>
                  )}
                </td>
                <td>{policyLabel(job.dependency_policy)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function JobDetails({
  api,
  job,
  onJobUpdate,
}: {
  api: BuildDiagnosticsApi;
  job: JobResource;
  onJobUpdate: (job: JobEventTarget) => void;
}) {
  const failure =
    job.terminal === null
      ? 'Not terminal'
      : job.terminal.failure_classification === null
        ? 'None'
        : formatEnumLabel(job.terminal.failure_classification);
  return (
    <section aria-label="Selected Job" className={styles.panel}>
      <div className={styles.panelHeading}>
        <div>
          <p className={styles.eyebrow}>Selected Job</p>
          <h2 id="selected-job-heading">{job.pipeline_node_id}</h2>
          <p className={styles.monospace}>{job.id}</p>
        </div>
        <Status state={job.state} />
      </div>
      <dl className={styles.jobDetails}>
        <Definition label="State" value={stateLabel(job.state)} />
        <Definition label="Failure classification" value={failure} />
        <Definition label="Dependency policy" value={policyLabel(job.dependency_policy)} />
        <Definition label="Dependencies" value={String(job.dependency_job_ids.length)} />
        <Definition label="Event cursor" value={String(job.event_cursor)} />
        <Definition
          label="Assigned Agent"
          value={job.assignment?.assigned_agent_id ?? 'Unassigned'}
        />
        <Definition
          label="Selected pool"
          value={job.assignment?.selected_pool_id ?? 'Unassigned'}
        />
        <Definition label="Runtime" value={formatEnumLabel(job.placement.runtime_class)} />
      </dl>
      <JobEvents api={api} job={job} key={job.id} onJobUpdate={onJobUpdate} />
    </section>
  );
}

function Status({
  state,
}: {
  state: BuildResource['state'] | AttemptResource['attempt']['state'] | JobResource['state'];
}) {
  const family = stateFamily(state);
  return (
    <span className={`${styles.status} ${styles[`state_${family}`]}`}>
      {formatEnumLabel(state)}
    </span>
  );
}

function Definition({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function BuildLoading() {
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <header className={styles.heading}>
        <p className={styles.eyebrow}>Build diagnostics</p>
        <h1 id="page-title">Build</h1>
      </header>
      <DiagnosticLoading label="Build" />
    </section>
  );
}

function BuildNotFound() {
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <header className={styles.heading}>
        <p className={styles.eyebrow}>Build diagnostics</p>
        <h1 id="page-title">Build not found</h1>
        <p>The Build does not exist or is not visible from this management context.</p>
      </header>
    </section>
  );
}

function isNotFound(error: Error | null): boolean {
  return error instanceof ManagementApiError && error.code === 'not_found';
}
