import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { useCursorPage } from '../../shared/PagedExplorerBranch';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Parameters = components['schemas']['FactoryIssueStatusActionParameters'];
export type FlowIssueServices = { api: IssueWorkflowsApi; projectId: string };

/** Edits an installed profile's local command. Evidence stays in the existing typed input mapper. */
export function FlowIssueStatus({
  node,
  services,
  onChange,
}: {
  node: Node;
  services: FlowIssueServices;
  onChange: (node: Node) => void;
}) {
  const { t } = usePresentation();
  const [target, setTarget] = useState<{ id: string; version: number } | null>(null);
  const [version, setVersion] = useState('');
  const [transition, setTransition] = useState('');
  const parameters = node.action?.parameters;
  const [outcome, setOutcome] = useState(() =>
    parameters &&
    typeof parameters === 'object' &&
    'outcome' in parameters &&
    typeof parameters.outcome === 'string'
      ? parameters.outcome
      : '',
  );
  const listing = useCursorPage(queryKeys.issueWorkflows(services.projectId), (cursor, signal) =>
    services.api.list(services.projectId, cursor, signal),
  );
  const selected = useQuery({
    queryKey: queryKeys.issueWorkflow(services.projectId, target?.id ?? null, target?.version),
    queryFn: ({ signal }) =>
      services.api.get(services.projectId, target!.id, target!.version, signal),
    enabled: target !== null,
  });
  const workflow =
    target &&
    selected.data?.id === target.id &&
    selected.data.version === target.version &&
    selected.data.project_id === services.projectId
      ? selected.data
      : undefined;
  const workflows = listing.data?.pages.flatMap((page) => page.items) ?? [];
  // The catalog lists current versions; keep the exact retained version visible during review.
  if (
    workflow &&
    !workflows.some((item) => item.id === workflow.id && item.version === workflow.version)
  ) {
    workflows.push({ ...workflow, name: workflow.definition.name });
  }
  const edges =
    workflow?.definition.transitions.filter((edge) => edge.origins.includes('pipeline')) ?? [];
  const edge = edges.find((edge) => edge.id === transition);
  const valid =
    workflow &&
    edge &&
    node.outcomes.some((item) => item.key === outcome) &&
    Number(version) === workflow.version &&
    !listing.isError &&
    !selected.isError &&
    !selected.isFetching;
  const statusName = (id: string) =>
    workflow?.definition.statuses.find((status) => status.id === id)?.name ?? id;
  return (
    <section aria-label={t('factory.issueStatusAction')}>
      <h4>{t('factory.issueStatusAction')}</h4>
      <p>{t('factory.issueStatusHelp')}</p>
      {parameters &&
      typeof parameters === 'object' &&
      'workflow_id' in parameters &&
      'workflow_version' in parameters &&
      'transition_id' in parameters &&
      'outcome' in parameters ? (
        <p>
          {t('factory.issueStatusPinned', {
            workflow: String(parameters.workflow_id),
            version: String(parameters.workflow_version),
            transition: String(parameters.transition_id),
            outcome: String(parameters.outcome),
          })}
        </p>
      ) : null}
      {listing.isPending || selected.isFetching ? (
        <QueryLoadingNotice label={t('factory.issueWorkflow')} />
      ) : null}
      {listing.error || selected.error ? (
        <QueryFailureNotice
          title={t('factory.issueStatusAction')}
          error={listing.error ?? selected.error!}
          onRetry={() => {
            void listing.refetch();
            if (target) void selected.refetch();
          }}
        />
      ) : null}
      {listing.data &&
      listing.data.pages.every((page) => page.items.length === 0) &&
      !listing.isError ? (
        <p>{t('issues.noWorkflows')}</p>
      ) : null}
      <SelectMenu
        ariaLabel={t('factory.issueWorkflow')}
        value={target ? `${target.id}:${target.version}` : ''}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...workflows.map((item) => ({
            value: `${item.id}:${item.version}`,
            label: `${item.name} · v${item.version}`,
          })),
        ]}
        onValueChange={(value) => {
          const item = workflows.find((item) => `${item.id}:${item.version}` === value);
          setTarget(item ? { id: item.id, version: item.version } : null);
          setVersion(item ? String(item.version) : '');
          setTransition('');
        }}
      />
      {listing.hasNextPage ? (
        <button
          type="button"
          disabled={listing.isFetching}
          onClick={() => void listing.fetchNextPage()}
        >
          {t('workflow.more')}
        </button>
      ) : null}
      {target ? (
        <>
          <label>
            {t('factory.issueWorkflowVersion')}
            <input
              type="number"
              min={1}
              max={Number.MAX_SAFE_INTEGER}
              step={1}
              value={version}
              onChange={(event) => setVersion(event.target.value)}
            />
          </label>
          <button
            type="button"
            disabled={!Number.isSafeInteger(Number(version)) || Number(version) < 1}
            onClick={() => {
              setTarget({ ...target, version: Number(version) });
              setTransition('');
            }}
          >
            {t('factory.reviewWorkflowVersion')}
          </button>
        </>
      ) : null}
      <SelectMenu
        ariaLabel={t('factory.issueTransition')}
        value={transition}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...edges.map((edge) => ({
            value: edge.id,
            label: `${edge.name}: ${statusName(edge.from)} → ${statusName(edge.to)}`,
          })),
        ]}
        onValueChange={setTransition}
      />
      <SelectMenu
        ariaLabel={t('factory.issueOutcome')}
        value={outcome}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...node.outcomes.map((item) => ({ value: item.key, label: item.key })),
        ]}
        onValueChange={setOutcome}
      />
      {edge ? (
        <>
          <p>
            {edge.id} · {edge.from} → {edge.to}
          </p>
          <ul>
            {edge.evidence.map((requirement) => (
              <li key={requirement}>
                {t(
                  requirement === 'accepted_execution'
                    ? 'workflow.evidence.accepted_execution'
                    : 'workflow.evidence.passed_required_checks',
                )}
              </li>
            ))}
          </ul>
        </>
      ) : null}
      <p>{t('factory.issueEvidenceHelp')}</p>
      <button
        type="button"
        disabled={!valid}
        onClick={() => {
          if (!valid || !node.action) return;
          const parameters: Parameters = {
            workflow_id: workflow.id,
            workflow_version: workflow.version,
            transition_id: edge.id,
            outcome,
          };
          onChange({ ...node, action: { ...node.action, parameters } });
        }}
      >
        {t('factory.useIssueStatus')}
      </button>
    </section>
  );
}
