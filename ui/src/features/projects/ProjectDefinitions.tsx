import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { GitBranch, SlidersHorizontal, Workflow, Zap, type LucideIcon } from 'lucide-react';
import { useState, type ReactNode } from 'react';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type {
  BuildConfigurationResource,
  BuildConfigurationSummary,
  PipelineResource,
  PipelineSummary,
  ProjectDefinitionsApi,
  RepositoryResource,
  RepositorySummary,
  TriggerDefinitionDetails,
  TriggerDefinitionSummary,
} from './api';
import { formatEnumLabel } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { ManualBuildForm } from './ManualBuildForm';
import definitionStyles from './ProjectDefinitions.module.css';
import styles from './Projects.module.css';

interface ProjectDefinitionsProps {
  api: ProjectDefinitionsApi;
  projectId: string;
}

interface DefinitionSummary {
  id: string;
  published_at_unix_ms: number;
  version: number;
}

interface DefinitionPage<TItem> {
  items: TItem[];
  next_cursor: string | null;
}

interface DefinitionSectionProps<TItem extends DefinitionSummary, TDetails> {
  detailKey: (item: TItem) => readonly unknown[];
  emptyLabel: string;
  getDetails: (item: TItem, signal: AbortSignal) => Promise<TDetails>;
  getPage: (cursor: string | null, signal: AbortSignal) => Promise<DefinitionPage<TItem>>;
  headingId: string;
  icon: LucideIcon;
  itemLabel: (item: TItem) => string;
  queryKey: readonly unknown[];
  renderDetails: (details: TDetails) => ReactNode;
  renderTags?: (item: TItem) => ReactNode;
  title: string;
}

export function ProjectDefinitions({ api, projectId }: ProjectDefinitionsProps) {
  const { t } = usePresentation();
  return (
    <section aria-labelledby="definitions-heading" className={definitionStyles.section}>
      <div className={styles.sectionHeading}>
        <div>
          <p className={styles.eyebrow}>{t('projects.workspace')}</p>
          <h2 id="definitions-heading">{t('definitions.current')}</h2>
        </div>
      </div>
      <p className={styles.sectionDescription}>{t('definitions.description')}</p>
      <div className={definitionStyles.grid}>
        <DefinitionSection<PipelineSummary, PipelineResource>
          detailKey={(item) => queryKeys.pipeline(item.id, item.version)}
          emptyLabel={t('definitions.noPipelines')}
          getDetails={(item, signal) => api.getPipeline(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listPipelines(projectId, cursor, signal)}
          headingId="pipelines-heading"
          icon={Workflow}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectPipelines(projectId)}
          renderDetails={(details) => renderPipelineDetails(details, t)}
          title={t('definitions.pipelines')}
        />
        <DefinitionSection<RepositorySummary, RepositoryResource>
          detailKey={(item) => queryKeys.repository(item.id, item.version)}
          emptyLabel={t('definitions.noRepositories')}
          getDetails={(item, signal) => api.getRepository(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listRepositories(projectId, cursor, signal)}
          headingId="repositories-heading"
          icon={GitBranch}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectRepositories(projectId)}
          renderDetails={(details) => renderRepositoryDetails(details, t)}
          title={t('definitions.repositories')}
        />
        <DefinitionSection<BuildConfigurationSummary, BuildConfigurationResource>
          detailKey={(item) => queryKeys.buildConfiguration(item.id, item.version)}
          emptyLabel={t('definitions.noConfigurations')}
          getDetails={(item, signal) => api.getBuildConfiguration(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listBuildConfigurations(projectId, cursor, signal)}
          headingId="build-configurations-heading"
          icon={SlidersHorizontal}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectBuildConfigurations(projectId)}
          renderDetails={(configuration) => (
            <BuildConfigurationDetails
              api={api}
              configuration={configuration}
              projectId={projectId}
            />
          )}
          renderTags={(item) => <EnabledTag enabled={item.enabled} />}
          title={t('definitions.configurations')}
        />
        <DefinitionSection<TriggerDefinitionSummary, TriggerDefinitionDetails>
          detailKey={(item) => queryKeys.trigger(item.kind, item.id, item.version)}
          emptyLabel={t('definitions.noTriggers')}
          getDetails={(item, signal) => api.getTrigger(item, signal)}
          getPage={(cursor, signal) => api.listTriggers(projectId, cursor, signal)}
          headingId="triggers-heading"
          icon={Zap}
          itemLabel={(item) =>
            t('definitions.triggerLabel', { kind: formatEnumLabel(item.kind, t) })
          }
          queryKey={queryKeys.projectTriggers(projectId)}
          renderDetails={(details) => renderTriggerDetails(details, t)}
          renderTags={(item) => (
            <>
              <span className={definitionStyles.kindTag}>{formatEnumLabel(item.kind, t)}</span>
              <EnabledTag enabled={item.enabled} />
            </>
          )}
          title={t('definitions.triggers')}
        />
      </div>
    </section>
  );
}

function DefinitionSection<TItem extends DefinitionSummary, TDetails>({
  detailKey,
  emptyLabel,
  getDetails,
  getPage,
  headingId,
  icon: Icon,
  itemLabel,
  queryKey,
  renderDetails,
  renderTags,
  title,
}: DefinitionSectionProps<TItem, TDetails>) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<TItem | null>(null);
  const collection = useInfiniteQuery({
    getNextPageParam: (page: DefinitionPage<TItem>) => page.next_cursor ?? undefined,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => getPage(pageParam, signal),
    queryKey,
  });
  const details = useQuery({
    enabled: selected !== null,
    queryFn: ({ signal }) => getDetails(requireSelection(selected), signal),
    queryKey:
      selected === null ? queryKeys.disabledDefinitionDetail(queryKey) : detailKey(selected),
  });

  const items = collection.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <section aria-labelledby={headingId} className={definitionStyles.card}>
      <header className={definitionStyles.header}>
        <span aria-hidden="true" className={definitionStyles.icon}>
          <Icon size={18} strokeWidth={1.8} />
        </span>
        <h3 id={headingId}>{title}</h3>
        {collection.data === undefined ? null : (
          <span>{t('definitions.loaded', { count: items.length })}</span>
        )}
      </header>
      {collection.isPending ? (
        <QueryLoadingNotice className={definitionStyles.detailState} label={title} />
      ) : collection.data === undefined ? (
        <QueryFailureNotice
          className={styles.compactFailure}
          error={collection.error}
          onRetry={collection.refetch}
          title={t('definitions.loadFailure', { title })}
        />
      ) : items.length === 0 ? (
        <QueryEmptyNotice className={definitionStyles.empty}>{emptyLabel}</QueryEmptyNotice>
      ) : (
        <ul className={definitionStyles.list}>
          {items.map((item) => {
            const expanded = selected?.id === item.id && selected.version === item.version;
            const label = itemLabel(item);
            return (
              <li key={`${item.id}:${item.version}`}>
                <button
                  aria-label={
                    expanded
                      ? t('definitions.hideLabelDetails', { label })
                      : t('definitions.viewLabelDetails', { label })
                  }
                  aria-expanded={expanded}
                  className={definitionStyles.itemButton}
                  onClick={() => setSelected(expanded ? null : item)}
                  type="button"
                >
                  <span className={definitionStyles.summary}>
                    <strong>{label}</strong>
                    <span>{t('builds.version', { version: item.version })}</span>
                  </span>
                  <span className={definitionStyles.tags}>{renderTags?.(item)}</span>
                  <span className={definitionStyles.detailsAction}>
                    {expanded
                      ? t('definitions.hideDetails')
                      : t('definitions.viewDetails', { label })}
                  </span>
                </button>
                {expanded ? (
                  <DefinitionDetails
                    details={details.data}
                    error={details.error}
                    isFetching={details.isFetching}
                    isPending={details.isPending}
                    label={label}
                    onRetry={details.refetch}
                    render={renderDetails}
                  />
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
      {collection.data === undefined ? null : (
        <QueryBackgroundNotice
          className={styles.staleNotice}
          error={collection.error}
          fetching={collection.isFetching && !collection.isFetchingNextPage}
          label={t('definitions.data', { title })}
          onRetry={collection.isFetchNextPageError ? collection.fetchNextPage : collection.refetch}
        />
      )}
      {collection.hasNextPage ? (
        <button
          className={styles.loadMoreButton}
          disabled={collection.isFetchingNextPage}
          onClick={() => void collection.fetchNextPage()}
          type="button"
        >
          {collection.isFetchingNextPage
            ? t('definitions.loading', { title })
            : t('definitions.loadMore', { title })}
        </button>
      ) : null}
    </section>
  );
}

function DefinitionDetails<TDetails>({
  details,
  error,
  isFetching,
  isPending,
  label,
  onRetry,
  render,
}: {
  details: TDetails | undefined;
  error: Error | null;
  isFetching: boolean;
  isPending: boolean;
  label: string;
  onRetry: () => unknown;
  render: (details: TDetails) => ReactNode;
}) {
  const { t } = usePresentation();
  if (isPending) {
    return (
      <QueryLoadingNotice
        className={definitionStyles.detailState}
        label={t('definitions.labelDetails', { label })}
      />
    );
  }
  if (details === undefined) {
    return (
      <QueryFailureNotice
        className={styles.compactFailure}
        error={error}
        onRetry={onRetry}
        title={t('definitions.detailsLoadFailure', { label })}
      />
    );
  }
  return (
    <>
      <div className={definitionStyles.details}>{render(details)}</div>
      <QueryBackgroundNotice
        className={styles.staleNotice}
        error={error}
        fetching={isFetching}
        label={t('definitions.detailsData', { label })}
        onRetry={onRetry}
      />
    </>
  );
}

function EnabledTag({ enabled }: { enabled: boolean }) {
  const { t } = usePresentation();
  return (
    <span className={enabled ? definitionStyles.enabledTag : definitionStyles.disabledTag}>
      {enabled ? t('definitions.enabled') : t('definitions.disabled')}
    </span>
  );
}

type Translate = ReturnType<typeof usePresentation>['t'];

function renderPipelineDetails(pipeline: PipelineResource, t: Translate) {
  return (
    <DetailList
      items={[
        [t('definitions.jobs'), t('definitions.jobCount', { count: pipeline.dag.nodes.length })],
        [t('definitions.dependencies'), String(pipeline.dag.edges.length)],
      ]}
    />
  );
}

function renderRepositoryDetails(repository: RepositoryResource, t: Translate) {
  return (
    <DetailList
      items={[
        [t('definitions.locator'), repository.definition.repository_locator],
        [
          t('definitions.defaultReference'),
          repository.definition.selection.default_reference ?? t('common.none'),
        ],
        [
          t('definitions.allowedReferences'),
          String(repository.definition.selection.allowed_references.length),
        ],
      ]}
    />
  );
}

function BuildConfigurationDetails({
  api,
  configuration,
  projectId,
}: {
  api: ProjectDefinitionsApi;
  configuration: BuildConfigurationResource;
  projectId: string;
}) {
  const { t } = usePresentation();
  return (
    <div className={definitionStyles.configurationDetails}>
      <DetailList
        items={[
          [
            t('definitions.pipeline'),
            `${configuration.definition.pipeline_id} v${configuration.definition.pipeline_version}`,
          ],
          [
            t('definitions.repository'),
            `${configuration.definition.repository_id} v${configuration.definition.repository_version}`,
          ],
          [t('definitions.jobConcurrency'), String(configuration.definition.job_concurrency_limit)],
        ]}
      />
      <ManualBuildForm api={api} configuration={configuration} projectId={projectId} />
    </div>
  );
}

function renderTriggerDetails(details: TriggerDefinitionDetails, t: Translate) {
  if (details.kind === 'scheduled') {
    return (
      <DetailList
        items={[
          [t('definitions.schedule'), details.resource.schedule.expression],
          [t('definitions.timezone'), details.resource.schedule.timezone],
          [t('builds.configuration'), formatConfiguration(details.resource)],
        ]}
      />
    );
  }
  if (details.kind === 'internal') {
    return (
      <DetailList
        items={[
          [t('definitions.outcome'), formatEnumLabel(details.resource.outcome, t)],
          [
            t('definitions.upstreamConfiguration'),
            `${details.resource.upstream_configuration_id} v${details.resource.upstream_configuration_version}`,
          ],
          [t('builds.configuration'), formatConfiguration(details.resource)],
        ]}
      />
    );
  }
  return (
    <DetailList items={[[t('builds.configuration'), formatConfiguration(details.resource)]]} />
  );
}

function DetailList({ items }: { items: [string, string][] }) {
  return (
    <dl>
      {items.map(([term, value]) => (
        <div key={term}>
          <dt>{term}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function formatConfiguration(resource: {
  configuration_id: string;
  configuration_version: number;
}) {
  return `${resource.configuration_id} v${resource.configuration_version}`;
}

function requireSelection<TItem>(selection: TItem | null): TItem {
  if (selection === null) {
    throw new Error('definition details require a selected summary');
  }
  return selection;
}
