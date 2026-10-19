import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { projectPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import {
  PipelineTargetFields,
  usePipelineTarget,
  type PipelineTargetSources,
} from '../../shared/PipelineTargetFields';
import { SelectMenu } from '../../shared/SelectMenu';
import type { Issue } from './api';
import type { IssueExecutionsApi, IssueExecutionRequest } from './executionApi';
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
  sources: PipelineTargetSources;
  ready: boolean;
  onRefresh: () => Promise<unknown>;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const { t } = usePresentation();
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
  const [priority, setPriority] = useState<number>(classification.priority.minimum);
  const [risk, setRisk] = useState<IssueExecutionRequest['classification']['risk']>(
    classification.risk.enum[0],
  );
  const [review, setReview] = useState<{
    request: IssueExecutionRequest;
    version: number;
    consequence: string;
    returnFocus: HTMLElement;
  } | null>(null);
  const valid =
    ready &&
    selection.target !== null &&
    !!issue.content.acceptance?.trim() &&
    Number.isSafeInteger(priority) &&
    priority >= classification.priority.minimum &&
    priority <= classification.priority.maximum;
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
        />
      </fieldset>
      <div className={styles.actions}>
        <button
          disabled={!valid || review !== null}
          type="button"
          onClick={(event) => {
            if (!selection.target) return;
            setReview({
              request: {
                target: { ...selection.target, mapping: 'title_description_acceptance_v1' },
                classification: { priority, risk, metadata: {} },
              },
              version: issue.version,
              returnFocus: event.currentTarget,
              consequence: `${t('issues.executionReview', { version: issue.version })} ${selection.summary}; ${t('factory.workPriority')}: ${priority}; ${t('factory.workRisk')}: ${risk}\n${issue.content.title}\n${issue.content.description}\n${issue.content.acceptance}`,
            });
          }}
        >
          {t('issues.reviewExecution')}
        </button>
        <button type="button" disabled={review !== null} onClick={onClose}>
          {t('command.abandon')}
        </button>
      </div>
      {review ? (
        <ConfirmedCommand
          title={t('issues.reviewExecution')}
          confirmLabel={t('issues.confirmExecution')}
          consequence={review.consequence}
          request={review.request}
          version={review.version}
          ready={valid && issue.version === review.version}
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
