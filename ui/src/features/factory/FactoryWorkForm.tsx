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
import type { FactoryApi } from './api';

/** Read ports supplied by application composition; Factory does not import other feature internals. */
export interface FactoryWorkSources extends Pick<ProjectHierarchyApi, 'getProject'> {
  listRepositories(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['RepositorySummaryPage']>;
  getRepository(
    repositoryId: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<components['schemas']['RepositoryResource']>;
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
  const [configurationId, setConfigurationId] = useState('');
  const [repositoryId, setRepositoryId] = useState('');
  const [buildId, setBuildId] = useState('');
  const [label, setLabel] = useState('');
  const [sourceSelection, setSourceSelection] = useState('default');
  const [revision, setRevision] = useState('');
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
  const configurations = useCursorPage(
    queryKeys.factoryConfigurations(projectId),
    (cursor, signal) => api.listConfigurations(projectId, cursor, signal),
  );
  const repositories = useCursorPage(
    queryKeys.factoryWorkSources(projectId, 'repositories'),
    (cursor, signal) => sources.listRepositories(projectId, cursor, signal),
  );
  const builds = useCursorPage(
    queryKeys.factoryWorkSources(projectId, 'builds'),
    (cursor, signal) =>
      sources.listBuilds(projectId, { configurationId: null, state: null }, cursor, signal),
  );
  const configurationItems =
    configurations.data?.pages.flatMap((page) => page.items).filter((item) => item.enabled) ?? [];
  const repositoryItems = repositories.data?.pages.flatMap((page) => page.items) ?? [];
  const buildItems = builds.data?.pages.flatMap((page) => page.items) ?? [];
  const configuration =
    configurationItems.find((item) => item.id === configurationId) ?? configurationItems[0];
  const repository = repositoryItems.find((item) => item.id === repositoryId) ?? repositoryItems[0];
  const build = buildItems.find((item) => item.id === buildId) ?? buildItems[0];
  const repositoryDetails = useQuery({
    queryKey: queryKeys.repository(repository?.id ?? '', repository?.version ?? 0),
    enabled: repository !== undefined,
    queryFn: ({ signal }) => {
      if (!repository) throw new Error('Repository selection is missing');
      return sources.getRepository(repository.id, repository.version, signal);
    },
  });
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
  const selection = repositoryDetails.data?.definition.selection;
  const sourceOptions = [
    ...(selection?.default_reference
      ? [{ value: 'default', label: t('manualBuild.defaultReference') }]
      : []),
    ...(selection?.allowed_references.map((value) => ({ value: `ref:${value}`, label: value })) ??
      []),
    ...(selection?.allow_exact_revision
      ? [{ value: 'exact', label: t('manualBuild.exactRevision') }]
      : []),
  ];
  const selectedSource =
    sourceOptions.find((item) => item.value === sourceSelection)?.value ?? sourceOptions[0]?.value;
  const source: Request['source'] | null =
    selectedSource === 'default'
      ? { kind: 'default_reference' }
      : selectedSource === 'exact'
        ? revision.trim()
          ? { kind: 'exact_revision', value: revision.trim() }
          : null
        : selectedSource?.startsWith('ref:')
          ? { kind: 'reference', value: selectedSource.slice(4) }
          : null;
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
    ![
      configurations.error,
      repositories.error,
      builds.error,
      repositoryDetails.error,
      artifacts.error,
    ].some(Boolean);
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
        <PagedExplorerBranch
          query={configurations}
          empty={t('factory.unconfigured')}
          labels={labels(t('factory.configurations'))}
        >
          {() => (
            <SelectMenu
              ariaLabel={t('factory.configurations')}
              value={configuration?.id ?? ''}
              onValueChange={setConfigurationId}
              options={configurationItems.map((item) => ({
                value: item.id,
                label: `${t('factory.configurationVersion', { version: item.version })} · ${item.id}`,
              }))}
            />
          )}
        </PagedExplorerBranch>
        <PagedExplorerBranch
          query={repositories}
          empty={t('factory.noRepositories')}
          labels={labels(t('factory.repository'))}
        >
          {(items) => (
            <SelectMenu
              ariaLabel={t('factory.repository')}
              value={repository?.id ?? ''}
              onValueChange={setRepositoryId}
              options={items.map((item) => ({
                value: item.id,
                label: `${item.name} · v${item.version}`,
              }))}
            />
          )}
        </PagedExplorerBranch>
        {repositoryDetails.data ? (
          <QueryBackgroundNotice
            label={t('factory.repository')}
            error={repositoryDetails.error}
            fetching={repositoryDetails.isFetching}
            onRetry={repositoryDetails.refetch}
          />
        ) : repositoryDetails.error ? (
          <QueryFailureNotice
            title={t('factory.repository')}
            error={repositoryDetails.error}
            onRetry={repositoryDetails.refetch}
          />
        ) : repository && repositoryDetails.isPending ? (
          <QueryLoadingNotice label={t('factory.repository')} />
        ) : null}
        <SelectMenu
          ariaLabel={t('manualBuild.source')}
          value={selectedSource ?? ''}
          onValueChange={setSourceSelection}
          options={sourceOptions}
        />
        {selectedSource === 'exact' ? (
          <label>
            {t('manualBuild.exactRevision')}
            <input
              value={revision}
              onChange={(event) => setRevision(event.currentTarget.value)}
              autoComplete="off"
            />
          </label>
        ) : null}
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
                ? selection?.default_reference
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
