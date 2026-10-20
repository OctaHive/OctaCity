import { useRef, useState, type RefObject } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { ISSUE_SOURCE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { WorkflowResource } from '../../api/issueWorkflows';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { SourceReporting } from '../../shared/SourceReporting';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceBindingKey, sourceReferenceKey } from '../../shared/sourceReferences';
import type { Issue } from './api';
import { IssueSourcePullRules } from './IssueSourcePullRules';
import { IssueSourceProfileSelect } from './IssueSourceProfileSelect';
import {
  sourceAuthorityLabel,
  sourceFieldLabel,
  type SourceAuthority,
} from './issueSourcePresentation';
import type {
  IssueSourceChoice,
  IssueSourceMappingDefinition,
  IssueSourceMappingRequest,
  IssueSourcesApi,
  IssueSynchronization,
  IssueSourceProjection,
} from './sourceApi';

/** Reviews field translation against a pinned workflow; it cannot edit that workflow. */
export function IssueSourceMapping({
  issue,
  api,
  review,
  choices,
  workflow,
  ready,
  onReload,
  fallbackFocusRef,
  resolution,
}: {
  issue: Issue;
  api: IssueSourcesApi;
  review: IssueSynchronization;
  choices: readonly IssueSourceChoice[];
  workflow: WorkflowResource | undefined;
  ready: boolean;
  onReload: () => unknown;
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
  resolution?: IssueSourceProjection | null;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [baseline] = useState(() => structuredClone(review));
  const [originalResolution] = useState(() => (resolution ? structuredClone(resolution) : null));
  const [definition, setDefinition] = useState<IssueSourceMappingDefinition>(() =>
    structuredClone(
      review.mapping?.definition ?? {
        source: review.link!.selection.source,
        reporter: null,
        push_enabled: false,
        fields: {},
        status: null,
      },
    ),
  );
  const [reviewedChoices] = useState(() => JSON.stringify(choices));
  const choice = choices.find(
    (c) => sourceBindingKey(c.source) === sourceBindingKey(definition.source),
  );
  const originalPin = baseline.mapping_reference
    ? sourceReferenceKey(baseline.mapping_reference)
    : '';
  const currentPin = review.mapping_reference ? sourceReferenceKey(review.mapping_reference) : '';
  const fresh =
    ready &&
    originalResolution?.record_id === resolution?.record_id &&
    reviewedChoices === JSON.stringify(choices) &&
    review.issue_version === baseline.issue_version &&
    originalPin === currentPin &&
    review.workflow.id === baseline.workflow.id &&
    review.workflow.version === baseline.workflow.version &&
    workflow?.id === baseline.workflow.id &&
    workflow.version === baseline.workflow.version;
  const fields = [...new Set([...(choice?.pull_fields ?? []), ...(choice?.push_fields ?? [])])];
  const push = definition.fields.status === 'octacity_owned_push';
  const pullValid =
    definition.fields.status !== 'source_owned_pull' ||
    (!!definition.status?.pull.length &&
      definition.status.pull.every((r) =>
        workflow?.definition.statuses.some((s) => s.id === r.status_id),
      ));
  const pushValid =
    !push ||
    (!!definition.status?.push?.statuses.length &&
      definition.status.push.statuses.every((r) =>
        workflow?.definition.statuses.some((s) => s.id === r.status),
      ));
  const canReview =
    pushValid &&
    pullValid &&
    fresh &&
    !!choice &&
    (!push || (!!choice.writer && !!definition.status?.push?.statuses.length)) &&
    (!originalResolution ||
      (push &&
        definition.push_enabled &&
        definition.status?.push?.statuses.some(
          (e) =>
            e.status === issue.status_id &&
            (e.add.length > 0 || e.remove.length > 0 || e.state !== null),
        ) === true)) &&
    new TextEncoder().encode(JSON.stringify(definition)).length <= bounds.mappingMaximumBytes;
  const [requested, setRequested] = useState<IssueSourceMappingRequest | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  function authority(field: string, value: SourceAuthority) {
    setDefinition((current) => ({
      ...current,
      fields: { ...current.fields, [field]: value },
      ...(field === 'status'
        ? {
            reporter: value === 'octacity_owned_push' ? (choice?.writer?.binding ?? null) : null,
            push_enabled: false,
            status:
              value === 'unsynchronized'
                ? null
                : {
                    workflow: baseline.workflow,
                    pull: [],
                    push: value === 'octacity_owned_push' ? { statuses: [] } : null,
                  },
          }
        : {}),
    }));
  }
  return (
    <section aria-label={t('issueSource.mapping')}>
      <p>{t('issueSource.mappingHelp')}</p>
      <p>
        {workflow?.definition.name} · {t('issues.revision', { version: baseline.workflow.version })}
      </p>
      <Link to={`/projects/${issue.project_id}/issue-workflows`}>{t('issues.workflow')}</Link>
      {!fresh ? <p role="alert">{t('issueSource.stale')}</p> : null}
      <button type="button" onClick={onReload}>
        {t('issueSource.reload')}
      </button>
      <fieldset disabled={!fresh}>
        <legend>{t('issueSource.mapping')}</legend>
        <IssueSourceProfileSelect
          choices={choices}
          value={sourceBindingKey(definition.source)}
          onChange={(key) => {
            const next = choices.find((c) => sourceBindingKey(c.source) === key);
            if (next)
              setDefinition({
                source: next.source,
                fields: {},
                status: null,
                reporter: null,
                push_enabled: false,
              });
          }}
        />
        {!choice ? <p role="alert">{t('issueSource.noProfiles')}</p> : null}
        {fields.map((field) => (
          <SelectMenu
            key={field}
            ariaLabel={t('issueSource.authority', { field: sourceFieldLabel(field, t) })}
            value={definition.fields[field] ?? 'unsynchronized'}
            options={[
              { value: 'unsynchronized', label: sourceAuthorityLabel('unsynchronized', t) },
              ...(choice?.pull_fields.includes(field)
                ? [
                    {
                      value: 'source_owned_pull',
                      label: sourceAuthorityLabel('source_owned_pull', t),
                    },
                  ]
                : []),
              ...(choice?.push_fields.includes(field)
                ? [
                    {
                      value: 'octacity_owned_push',
                      label: sourceAuthorityLabel('octacity_owned_push', t),
                    },
                  ]
                : []),
            ]}
            onValueChange={(value) => authority(field, value as SourceAuthority)}
          />
        ))}
        {definition.fields.status === 'source_owned_pull' &&
        definition.status &&
        choice &&
        workflow ? (
          <IssueSourcePullRules
            rows={definition.status.pull}
            choice={choice}
            workflow={workflow}
            onChange={(pull) => setDefinition((d) => ({ ...d, status: { ...d.status!, pull } }))}
          />
        ) : null}
        {push && choice?.writer && workflow ? (
          <>
            <SourceReporting
              mapping={undefined}
              policy={choice.writer.effects}
              draft={definition.status?.push ?? null}
              statusChoices={workflow.definition.statuses.map((s) => ({
                value: s.id,
                label: s.name,
              }))}
              maximumRows={bounds.pushMaximumRows}
              onChange={(mapping) =>
                setDefinition((d) => ({ ...d, status: { ...d.status!, push: mapping } }))
              }
            />
            <label>
              <input
                type="checkbox"
                checked={definition.push_enabled}
                onChange={(e) => setDefinition((d) => ({ ...d, push_enabled: e.target.checked }))}
              />
              {t('issueSource.enablePush')}
            </label>
          </>
        ) : null}
        <p>{t('issueSource.localFields')}</p>
        <button
          type="button"
          ref={triggerRef}
          disabled={!canReview}
          onClick={() =>
            setRequested({
              expected_mapping: baseline.mapping_reference,
              definition: structuredClone(definition),
            })
          }
        >
          {t(originalResolution ? 'issueSync.reviewPolicy' : 'issueSource.reviewMapping')}
        </button>
      </fieldset>
      {requested ? (
        <ConfirmedCommand
          title={t(originalResolution ? 'issueSync.reviewPolicy' : 'issueSource.reviewMapping')}
          confirmLabel={t(
            originalResolution ? 'issueSync.publishResolution' : 'issueSource.publishMapping',
          )}
          consequence={
            (originalResolution
              ? t('issueSync.resolutionConsequence', {
                  operation: originalResolution.operation_id,
                  receipt: originalResolution.record_id,
                }) + ' '
              : '') +
            t('issueSource.mappingConsequence', {
              workflow: workflow?.definition.name ?? baseline.workflow.id,
              version: baseline.workflow.version,
              fields: Object.entries(requested.definition.fields)
                .map(([f, a]) => `${sourceFieldLabel(f, t)}: ${sourceAuthorityLabel(a, t)}`)
                .join('; '),
            }) +
            (requested.definition.status?.pull
              .map(
                (r) =>
                  ` ${workflow?.definition.statuses.find((s) => s.id === r.status_id)?.name ?? r.status_id}: ${r.native.state ?? t('issueSource.anyState')} [${r.native.all_of.join(', ')}; ${r.native.any_of.join(', ')}; ${r.native.none_of.join(', ')}]`,
              )
              .join('; ') ?? '') +
            (requested.definition.status?.push?.statuses
              .map(
                (r) =>
                  ` ${workflow?.definition.statuses.find((s) => s.id === r.status)?.name ?? r.status}: +${r.add.join(', ')} -${r.remove.join(', ')} →${r.state ?? t('factory.preserveState')}`,
              )
              .join('; ') ?? '') +
            (requested.definition.push_enabled ? ' · ' + t('issueSource.enablePush') : '')
          }
          request={requested}
          version={baseline.issue_version}
          ready={canReview}
          execute={async ({ request, headers }) =>
            originalResolution
              ? await api.resolve(
                  issue.project_id,
                  issue.id,
                  {
                    mapping: structuredClone(request) as IssueSourceMappingRequest,
                    review: {
                      operation_id: originalResolution.operation_id,
                      expected_record: originalResolution.record_id,
                    },
                  },
                  requireVersionedMutationHeaders(headers),
                )
              : await api.publishMapping(
                  issue.project_id,
                  issue.id,
                  structuredClone(request) as IssueSourceMappingRequest,
                  requireVersionedMutationHeaders(headers),
                )
          }
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issueRoot(issue.project_id, issue.id) }]}
          onClose={() => setRequested(null)}
          onRefresh={onReload}
          returnFocus={
            canReview ? triggerRef.current! : (fallbackFocusRef.current ?? triggerRef.current!)
          }
          fallbackFocusRef={fallbackFocusRef}
          renderSuccess={(result) => (
            <p>{t('issueSource.mappingPublished', { version: result.resource.version })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
