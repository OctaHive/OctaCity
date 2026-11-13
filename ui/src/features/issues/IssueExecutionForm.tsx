import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { projectPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { PipelineTargetFields, usePipelineTarget } from '../../shared/PipelineTargetFields';
import { SelectMenu } from '../../shared/SelectMenu';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import type { Issue } from './api';
import type {
  IssueExecutionsApi,
  IssueExecutionRequest,
  IssueExecutionSources,
} from './executionApi';
import { sameExecutionTarget } from './issueExecutionTarget';
import { IssueExecutionSnapshotView } from './IssueExecutionSnapshotView';
import { IssueSnapshotProblems } from '../../shared/IssueSnapshotProblems';
import { QueryContent, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { ExecutionReceipt } from './IssueExecutionReceipt';
import styles from './Issues.module.css';
const classification = FACTORY_AUTHORING_SCHEMAS.FactoryWorkClassification.properties;

/** Only the confirmed HTTP command publishes inputs. Drafts and continuation keys stay in memory. */
export function IssueExecutionForm({
  issue,
  api,
  sources,
  ready,
  onRefresh,
  onSelect,
  onClose,
}: {
  issue: Issue;
  api: IssueExecutionsApi;
  sources: IssueExecutionSources;
  ready: boolean;
  onRefresh: () => Promise<unknown>;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const { t, locale } = usePresentation();
  const queryClient = useQueryClient();
  const activeRef = useRef(true);
  // Publication inputs stay frozen; selection follows the card's current section and revision.
  const selectCurrentRef = useRef(onSelect);
  selectCurrentRef.current = onSelect;
  useEffect(() => {
    activeRef.current = true;
    return () => {
      activeRef.current = false;
    };
  }, []);
  const selection = usePipelineTarget(issue.project_id, sources);
  const configuration = useQuery({
    queryKey: queryKeys.factoryConfiguration(
      selection.configuration?.id ?? null,
      selection.configuration?.version ?? null,
    ),
    enabled: !!selection.configuration,
    queryFn: ({ signal }) =>
      sources.getConfiguration(
        selection.configuration!.id,
        selection.configuration!.version,
        signal,
      ),
  });
  const flow =
    configuration.data?.configuration.reference.id === selection.configuration?.id &&
    configuration.data?.configuration.reference.version === selection.configuration?.version &&
    configuration.data?.configuration.reference.project_id === issue.project_id
      ? configuration.data.configuration.flow
      : undefined;
  const snapshotInput = flow?.issue_snapshot;
  const usePredecessor = !!flow?.accepted_predecessor;
  const [selectedRun, setSelectedRun] = useState('');
  const history = useCursorPage(
    queryKeys.issueExecutionPredecessors(issue.project_id, issue.id),
    (cursor, signal) => api.list(issue.project_id, issue.id, cursor, signal),
    { enabled: usePredecessor },
  );
  const predecessorOptions = new Map(
    (history.data?.pages.flatMap((page) => page.items) ?? []).flatMap((entry) => {
      const link = entry.association;
      return entry.state === 'admitted' &&
        link?.project_id === issue.project_id &&
        link.issue_id === issue.id &&
        entry.intent.target.repository_id === selection.repository?.id
        ? [
            [
              link.run_id,
              `${new Date(entry.requested_at).toLocaleString(locale)} · ${entry.id}`,
            ] as const,
          ]
        : [];
    }),
  );
  const predecessor = usePredecessor && predecessorOptions.has(selectedRun) ? selectedRun : '';
  const [priority, setPriority] = useState<number>(classification.priority.minimum);
  const [risk, setRisk] = useState<NonNullable<IssueExecutionRequest['classification']>['risk']>(
    classification.risk.enum[0],
  );
  const [review, setReview] = useState<{
    request: IssueExecutionRequest;
    version: number;
    consequence: string;
    returnFocus: HTMLElement;
  } | null>(null);
  const preview = useQuery({
    queryKey: queryKeys.issueExecutionReview(
      issue.project_id,
      issue.id,
      review?.version,
      review?.request,
    ),
    enabled: review !== null,
    gcTime: 0,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    queryFn: ({ signal }) =>
      api.review(issue.project_id, issue.id, review!.request, review!.version, signal),
  });
  const previewMatches = !!(
    review &&
    preview.data &&
    preview.data.version === review.version &&
    sameExecutionTarget(preview.data.target, review.request.target) &&
    (!snapshotInput || (preview.data.snapshot && preview.data.snapshot_digest)) &&
    (!preview.data.snapshot ||
      (preview.data.snapshot.project_id === issue.project_id &&
        preview.data.snapshot.issue_id === issue.id &&
        preview.data.snapshot.version === review.version &&
        sameExecutionTarget(preview.data.snapshot.target, review.request.target)))
  );
  const valid =
    ready &&
    selection.target !== null &&
    !!flow &&
    !configuration.isFetching &&
    !configuration.error &&
    (!usePredecessor || (!!predecessor && !history.isFetching && !history.error)) &&
    !!issue.content.acceptance?.trim() &&
    (!!snapshotInput?.admission ||
      (Number.isSafeInteger(priority) &&
        priority >= classification.priority.minimum &&
        priority <= classification.priority.maximum));
  return (
    <section className={styles.panel} aria-label={t('issues.start')}>
      <h2>{t('issues.start')}</h2>
      <p>{t('issues.executionInputHelp')}</p>
      <p>
        <Link to={`/factory?project_id=${encodeURIComponent(issue.project_id)}`}>
          {t('issues.configurePipeline')}
        </Link>
        {' · '}
        <Link to={`${projectPath(issue.project_id)}?section=repositories`}>
          {t('factory.repository')}
        </Link>
      </p>
      <fieldset disabled={review !== null}>
        <PipelineTargetFields selection={selection} />
        {selection.configuration ? (
          <QueryContent query={configuration} label={t('factory.configurations')}>
            {() => (flow ? null : <p role="alert">{t('issues.snapshotMismatch')}</p>)}
          </QueryContent>
        ) : null}
        {usePredecessor ? (
          <PagedExplorerBranch
            query={history}
            empty={t('issues.noPredecessors')}
            labels={{
              loading: t('issues.predecessorRun'),
              failure: t('issues.predecessorRun'),
              stale: t('issues.predecessorRun'),
              loadMore: t('issues.nextPage'),
              loadingMore: t('issues.predecessorRun'),
            }}
          >
            {() => (
              <>
                <p>{t('issues.predecessorHelp')}</p>
                <SelectMenu
                  ariaLabel={t('issues.predecessorRun')}
                  value={predecessor}
                  options={[
                    { value: '', label: t('factory.chooseInput') },
                    ...Array.from(predecessorOptions, ([value, label]) => ({ value, label })),
                  ]}
                  onValueChange={setSelectedRun}
                />
              </>
            )}
          </PagedExplorerBranch>
        ) : null}
        {!snapshotInput?.admission ? (
          <>
            {' '}
            <label>
              {t('factory.workPriority')}
              <input
                type="number"
                min={classification.priority.minimum}
                max={classification.priority.maximum}
                value={priority}
                onChange={(event) => setPriority(event.currentTarget.valueAsNumber)}
              />
            </label>
            <SelectMenu
              ariaLabel={t('factory.workRisk')}
              value={risk}
              onValueChange={setRisk}
              options={classification.risk.enum.map((value) => ({ value, label: value }))}
            />{' '}
          </>
        ) : null}
      </fieldset>
      <div className={styles.actions}>
        <button
          disabled={!valid || review !== null}
          type="button"
          onClick={(event) => {
            if (!selection.target) return;
            setReview({
              request: {
                target: {
                  ...selection.target,
                  mapping: 'title_description_acceptance_v1',
                  ...(predecessor ? { predecessor_run: predecessor } : {}),
                },
                ...(!snapshotInput?.admission
                  ? { classification: { priority, risk, metadata: {} } }
                  : {}),
              },
              version: issue.version,
              returnFocus: event.currentTarget,
              consequence: `${t('issues.executionReview', { version: issue.version })} ${selection.summary}; ${!snapshotInput?.admission ? `${t('factory.workPriority')}: ${priority}; ${t('factory.workRisk')}: ${risk}` : ''}${predecessor ? `\n${t('issues.predecessorRun')} · ${predecessor}` : ''}\n${issue.content.title}\n${issue.content.description}\n${issue.content.acceptance}`,
            });
          }}
        >
          {t('issues.reviewExecution')}
        </button>
        <button type="button" disabled={review !== null} onClick={onClose}>
          {t('command.abandon')}
        </button>
      </div>
      {review && preview.isPending ? (
        <>
          <QueryLoadingNotice label={t('issues.snapshotPreview')} />
          <button type="button" onClick={() => setReview(null)}>
            {t('issues.snapshotCancelReview')}
          </button>
        </>
      ) : null}
      {review && !preview.isPending ? (
        <ConfirmedCommand
          title={t('issues.reviewExecution')}
          confirmLabel={t('issues.confirmExecution')}
          consequence={review.consequence}
          renderReview={
            <>
              {issue.version !== review.version ? (
                <p role="alert">{t('issues.snapshotRevisionChanged')}</p>
              ) : null}
              <IssueSnapshotProblems error={preview.error} />
              <QueryContent query={preview} label={t('issues.snapshotPreview')}>
                {(data) =>
                  previewMatches && !preview.error && !preview.isFetching && data.snapshot ? (
                    <IssueExecutionSnapshotView
                      snapshot={data.snapshot}
                      digest={data.snapshot_digest}
                    />
                  ) : !previewMatches ? (
                    <p role="alert">{t('issues.snapshotMismatch')}</p>
                  ) : null
                }
              </QueryContent>
            </>
          }
          renderFailure={(error) => <IssueSnapshotProblems error={error} />}
          request={
            preview.data?.snapshot_digest
              ? { ...review.request, expected_snapshot_digest: preview.data.snapshot_digest }
              : review.request
          }
          version={review.version}
          ready={
            valid &&
            issue.version === review.version &&
            !!preview.data &&
            !preview.isFetching &&
            !preview.error &&
            previewMatches
          }
          returnFocus={review.returnFocus}
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issueExecutionsRoot(issue.project_id, issue.id) }]}
          execute={async (attempt) => {
            const result = await api.start(
              issue.project_id,
              issue.id,
              attempt.request,
              requireVersionedMutationHeaders(attempt.headers),
            );
            if (activeRef.current) selectCurrentRef.current(result.resource.id);
            return result;
          }}
          continuation={{
            isComplete: (result) => result.resource.state === 'admitted',
            resumeLabel: t('issues.resumeExecution'),
            renderPending: (result) => (
              <>
                <ExecutionReceipt execution={result.resource} currentVersion={issue.version} />
                <p>{t('issues.resumeMemoryHelp')}</p>
              </>
            ),
          }}
          renderSuccess={(result) => (
            <ExecutionReceipt execution={result.resource} currentVersion={issue.version} />
          )}
          onClose={() => setReview(null)}
          onRefresh={onRefresh}
        />
      ) : null}
    </section>
  );
}
