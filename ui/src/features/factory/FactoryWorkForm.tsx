import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Navigate } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import type { ProjectHierarchyApi } from '../../shared/projectHierarchy';
import { useCursorPage, PagedExplorerBranch } from '../../shared/PagedExplorerBranch';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import {
  PipelineTargetFields,
  usePipelineTarget,
  type PipelineTargetSources,
} from '../../shared/PipelineTargetFields';
import type { FactoryApi } from './api';

/** Read ports supplied by application composition; Factory does not import other feature internals. */
export interface FactoryWorkSources
  extends
    Pick<ProjectHierarchyApi, 'getProject'>,
    Pick<PipelineTargetSources, 'listRepositories' | 'getRepository'> {
  listBuilds(
    projectId: string,
    filters: { configurationId: string | null; state: components['schemas']['BuildState'] | null },
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['BuildSummaryPage']>;
  listBuildArtifacts(
    buildId: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['ArtifactPage']>;
}
type Request = components['schemas']['AdmitManualFactoryWorkRequest'];
type Artifact = components['schemas']['ArtifactResource'];
const workBounds = FACTORY_AUTHORING_SCHEMAS.AdmitManualFactoryWorkRequest.properties;
const classification = FACTORY_AUTHORING_SCHEMAS.FactoryWorkClassification.properties;
const artifactReference = (artifact: Artifact) => ({
  artifact_id: artifact.id,
  content_digest: artifact.sha256,
  encoded_size: artifact.size_bytes,
});

/** Uses visible immutable input references; unfinished Work exists only in this mounted form. */
export function FactoryWorkForm({
  api,
  projectId,
  onDiscard,
  sources,
}: {
  api: FactoryApi;
  projectId: string;
  onDiscard: () => void;
  sources: FactoryWorkSources;
}) {
  const { t, locale } = usePresentation();
  const queryClient = useQueryClient();
  const [buildId, setBuildId] = useState('');
  const [label, setLabel] = useState('');
  const [taskId, setTaskId] = useState('');
  const [acceptanceId, setAcceptanceId] = useState('');
  const [specificationIds, setSpecificationIds] = useState<string[]>([]);
  const [priority, setPriority] = useState<number>(classification.priority.minimum);
  const [risk, setRisk] = useState<Request['classification']['risk']>(classification.risk.enum[0]);
  const [review, setReview] = useState<{
    request: Request;
    returnFocus: HTMLElement;
    consequence: string;
  } | null>(null);
  const project = useQuery({
    queryKey: queryKeys.project(projectId),
    queryFn: ({ signal }) => sources.getProject(projectId, signal),
  });
  const targetSelection = usePipelineTarget(projectId, {
    ...sources,
    listConfigurations: api.listConfigurations,
  });
  const { configuration, repository, details: repositoryDetails } = targetSelection;
  const source = targetSelection.target?.source ?? null;
  const builds = useCursorPage(
    queryKeys.factoryWorkSources(projectId, 'builds'),
    (cursor, signal) =>
      sources.listBuilds(projectId, { configurationId: null, state: null }, cursor, signal),
  );
  const buildItems = builds.data?.pages.flatMap((page) => page.items) ?? [];
  const build = buildItems.find((item) => item.id === buildId) ?? buildItems[0];
  const artifacts = useQuery({
    queryKey: queryKeys.buildArtifacts(build?.id ?? ''),
    enabled: build !== undefined,
    queryFn: ({ signal }) => {
      if (!build) throw new Error('Build selection is missing');
      return sources.listBuildArtifacts(build.id, signal);
    },
  });
  const artifactItems = artifacts.data?.items ?? [];
  const specifications = artifactItems.filter((item) => specificationIds.includes(item.id));
  const specificationMaximum =
    FACTORY_AUTHORING_SCHEMAS.FactoryWorkArtifacts.properties.specifications.maxItems;
  const task = artifactItems.find((item) => item.id === taskId);
  const acceptance = artifactItems.find((item) => item.id === acceptanceId);
  const valid =
    project.data &&
    project.error === null &&
    configuration &&
    repository &&
    repositoryDetails.data?.project_id === projectId &&
    task &&
    acceptance &&
    source &&
    label.trim().length > 0 &&
    new TextEncoder().encode(label).length <= workBounds.external_identity.maxLength &&
    Number.isSafeInteger(priority) &&
    priority >= classification.priority.minimum &&
    priority <= classification.priority.maximum &&
    ![builds.error, repositoryDetails.error, artifacts.error].some(Boolean);
  const labels = (name: string) => ({
    failure: name,
    loading: name,
    stale: name,
    loadMore: t('factory.moreWorkSources'),
    loadingMore: name,
  });
  return (
    <section>
      <h2>{t('factory.admitWork')}</h2>
      {project.data ? (
        <>
          <p>{project.data.project.name}</p>
          <QueryBackgroundNotice
            label={t('section.projects')}
            error={project.error}
            fetching={project.isFetching}
            onRetry={project.refetch}
          />
        </>
      ) : project.isPending ? (
        <QueryLoadingNotice label={t('section.projects')} />
      ) : (
        <QueryFailureNotice
          title={t('section.projects')}
          error={project.error}
          onRetry={project.refetch}
        />
      )}
      <p>{t('factory.workArtifactsHelp')}</p>
      <button type="button" disabled={review !== null} onClick={onDiscard}>
        {t('factory.discardWork')}
      </button>
      <fieldset disabled={review !== null}>
        <label>
          {t('factory.workLabel')}
          <input
            value={label}
            maxLength={workBounds.external_identity.maxLength}
            onChange={(event) => setLabel(event.currentTarget.value)}
            autoComplete="off"
          />
        </label>
        <PipelineTargetFields selection={targetSelection} />
        <PagedExplorerBranch
          query={builds}
          empty={t('factory.noInputBuilds')}
          labels={labels(t('factory.inputBuild'))}
        >
          {(items) => (
            <SelectMenu
              ariaLabel={t('factory.inputBuild')}
              value={build?.id ?? ''}
              onValueChange={setBuildId}
              options={items.map((item) => ({
                value: item.id,
                label: `${new Date(item.created_at_unix_ms).toLocaleString(locale)} · ${item.state} · ${item.id}`,
              }))}
            />
          )}
        </PagedExplorerBranch>
        {artifacts.data ? (
          <QueryBackgroundNotice
            label={t('factory.workArtifacts')}
            error={artifacts.error}
            fetching={artifacts.isFetching}
            onRetry={artifacts.refetch}
          />
        ) : artifacts.error ? (
          <QueryFailureNotice
            title={t('factory.workArtifacts')}
            error={artifacts.error}
            onRetry={artifacts.refetch}
          />
        ) : build && artifacts.isPending ? (
          <QueryLoadingNotice label={t('factory.workArtifacts')} />
        ) : null}
        <SelectMenu
          ariaLabel={t('factory.taskArtifact')}
          value={taskId}
          onValueChange={setTaskId}
          options={artifactItems.map((item) => ({ value: item.id, label: item.name }))}
        />
        <SelectMenu
          ariaLabel={t('factory.acceptanceArtifact')}
          value={acceptanceId}
          onValueChange={setAcceptanceId}
          options={artifactItems.map((item) => ({ value: item.id, label: item.name }))}
        />
        <fieldset>
          <legend>{t('factory.specificationArtifacts')}</legend>
          {artifactItems.map((item) => (
            <label key={item.id}>
              <input
                type="checkbox"
                checked={specificationIds.includes(item.id)}
                disabled={
                  !specificationIds.includes(item.id) &&
                  specifications.length >= specificationMaximum
                }
                onChange={(event) => {
                  const checked = event.currentTarget.checked;
                  setSpecificationIds((current) =>
                    checked ? [...current, item.id] : current.filter((id) => id !== item.id),
                  );
                }}
              />
              {item.name}
            </label>
          ))}
        </fieldset>
        <label>
          {t('factory.workPriority')}
          <input
            type="number"
            min={classification.priority.minimum}
            max={classification.priority.maximum}
            value={priority}
            onChange={(event) => setPriority(Number(event.currentTarget.value))}
          />
        </label>
        <SelectMenu
          ariaLabel={t('factory.workRisk')}
          value={risk}
          onValueChange={(value) => {
            const selected = classification.risk.enum.find((item) => item === value);
            if (selected) setRisk(selected);
          }}
          options={classification.risk.enum.map((value) => ({ value, label: value }))}
        />
        <button
          type="button"
          disabled={!valid}
          onClick={(event) => {
            if (!valid || !configuration || !repository || !task || !acceptance || !source) return;
            const request: Request = {
              configuration_id: configuration.id,
              configuration_version: configuration.version,
              repository_id: repository.id,
              repository_version: repository.version,
              external_identity: label.trim(),
              source,
              artifacts: {
                task: artifactReference(task),
                acceptance: artifactReference(acceptance),
                specifications: specifications.map(artifactReference),
              },
              classification: { priority, risk, metadata: {} },
            };
            const reviewedSource =
              request.source.kind === 'default_reference'
                ? repositoryDetails.data?.definition.selection.default_reference
                : request.source.value;
            setReview({
              returnFocus: event.currentTarget,
              consequence: `${t('factory.workConsequence')} · ${t('factory.configurationVersion', { version: request.configuration_version })} · ${request.configuration_id} · ${repository.name} · v${request.repository_version} · ${reviewedSource} · ${task.name} · ${acceptance.name} · ${specifications.map((item) => item.name).join(' · ')}`,
              request,
            });
          }}
        >
          {t('factory.reviewWork')}
        </button>
      </fieldset>
      {review ? (
        <ConfirmedCommand
          title={t('factory.confirmWorkTitle')}
          consequence={review.consequence}
          confirmLabel={t('factory.confirmWork')}
          request={review.request}
          returnFocus={review.returnFocus}
          queryClient={queryClient}
          invalidations={[
            { queryKey: queryKeys.factoryRuns(projectId, {}) },
            { queryKey: queryKeys.factoryPoolRoot(projectId) },
            { queryKey: queryKeys.factoryRuns(null, {}) },
            { queryKey: queryKeys.audit },
          ]}
          execute={(attempt) =>
            api.admitWork(projectId, structuredClone(attempt.request) as Request, attempt.headers)
          }
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <Navigate to={`/factory/${encodeURIComponent(result.factory_run_id)}`} replace />
          )}
        />
      ) : null}
    </section>
  );
}
