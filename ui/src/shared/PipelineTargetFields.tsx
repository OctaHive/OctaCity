import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../.generated/api/schema';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { queryKeys } from '../app/query';
import { PagedExplorerBranch, useCursorPage } from './PagedExplorerBranch';
import { QueryBackgroundNotice, QueryFailureNotice, QueryLoadingNotice } from './QueryStateNotice';
import { SelectMenu } from './SelectMenu';

type Target = Pick<
  components['schemas']['AdmitManualFactoryWorkRequest'],
  'configuration_id' | 'configuration_version' | 'repository_id' | 'repository_version' | 'source'
>;
/** Existing authorized catalog reads are supplied by app composition, independently of intake. */
export interface PipelineTargetSources {
  listConfigurations(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryConfigurationSummaryPage']>;
  listRepositories(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['RepositorySummaryPage']>;
  getRepository(
    id: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<components['schemas']['RepositoryResource']>;
}

/** Pins visible configuration/repository versions and applies the repository's source policy. */
export function usePipelineTarget(projectId: string, sources: PipelineTargetSources) {
  const { t } = usePresentation();
  const [configurationId, setConfigurationId] = useState('');
  const [repositoryId, setRepositoryId] = useState('');
  const [sourceSelection, setSourceSelection] = useState('');
  const [revision, setRevision] = useState('');
  const configurations = useCursorPage(
    queryKeys.factoryConfigurations(projectId),
    (cursor, signal) => sources.listConfigurations(projectId, cursor, signal),
  );
  const repositories = useCursorPage(
    queryKeys.factoryWorkSources(projectId, 'repositories'),
    (cursor, signal) => sources.listRepositories(projectId, cursor, signal),
  );
  const configurationItems =
    configurations.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.enabled && item.project_id === projectId) ?? [];
  const repositoryItems =
    repositories.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.project_id === projectId) ?? [];
  // Do not silently substitute a different target after a selection disappears on refresh.
  const configuration = configurationId
    ? configurationItems.find((item) => item.id === configurationId)
    : configurationItems[0];
  const repository = repositoryId
    ? repositoryItems.find((item) => item.id === repositoryId)
    : repositoryItems[0];
  const details = useQuery({
    queryKey: queryKeys.repository(repository?.id ?? '', repository?.version ?? 0),
    enabled: !!repository,
    queryFn: ({ signal }) => sources.getRepository(repository!.id, repository!.version, signal),
  });
  const policy = details.data?.definition.selection;
  const sourceOptions = [
    ...(policy?.default_reference
      ? [
          {
            value: 'default',
            label: `${t('manualBuild.defaultReference')} · ${policy.default_reference}`,
          },
        ]
      : []),
    ...(policy?.allowed_references.map((value) => ({ value: `ref:${value}`, label: value })) ?? []),
    ...(policy?.allow_exact_revision
      ? [{ value: 'exact', label: t('manualBuild.exactRevision') }]
      : []),
  ];
  const selectedSource = sourceSelection
    ? sourceOptions.find((option) => option.value === sourceSelection)?.value
    : sourceOptions[0]?.value;
  const source: components['schemas']['ManualSource'] | null =
    selectedSource === 'default'
      ? { kind: 'default_reference' }
      : selectedSource === 'exact'
        ? revision.trim()
          ? { kind: 'exact_revision', value: revision.trim() }
          : null
        : selectedSource?.startsWith('ref:')
          ? { kind: 'reference', value: selectedSource.slice(4) }
          : null;
  const ready =
    !!(
      configuration &&
      repository &&
      source &&
      details.data?.id === repository.id &&
      details.data.version === repository.version &&
      details.data.project_id === projectId
    ) &&
    ![configurations, repositories, details].some(
      (query) => query.isFetching || query.error !== null,
    );
  const target: Target | null =
    ready && configuration && repository && source
      ? {
          configuration_id: configuration.id,
          configuration_version: configuration.version,
          repository_id: repository.id,
          repository_version: repository.version,
          source,
        }
      : null;
  const summary =
    configuration && repository && source
      ? `${configuration.id} · v${configuration.version}; ${repository.name} · v${repository.version}; ${source.kind === 'default_reference' ? policy?.default_reference : source.value}`
      : '';
  return {
    configurations,
    repositories,
    details,
    configurationItems,
    repositoryItems,
    configuration,
    repository,
    setConfigurationId,
    setRepositoryId,
    sourceOptions,
    selectedSource,
    setSourceSelection,
    revision,
    setRevision,
    target,
    summary,
  };
}

/** Shared keyboard-accessible fields and stale/empty/error feedback for pinned pipeline inputs. */
export function PipelineTargetFields({
  selection,
}: {
  selection: ReturnType<typeof usePipelineTarget>;
}) {
  const { t } = usePresentation();
  const { configurations, repositories, details } = selection;
  const labels = (name: string) => ({
    failure: name,
    loading: name,
    stale: name,
    loadMore: t('factory.moreWorkSources'),
    loadingMore: name,
  });
  return (
    <>
      <PagedExplorerBranch
        query={configurations}
        empty={t('factory.unconfigured')}
        labels={labels(t('factory.configurations'))}
      >
        {() =>
          selection.configurationItems.length ? (
            <SelectMenu
              ariaLabel={t('factory.configurations')}
              value={selection.configuration?.id ?? ''}
              onValueChange={selection.setConfigurationId}
              options={selection.configurationItems.map((item) => ({
                value: item.id,
                label: `${t('factory.configurationVersion', { version: item.version })} · ${item.id}`,
              }))}
            />
          ) : (
            <p>{t('factory.unconfigured')}</p>
          )
        }
      </PagedExplorerBranch>
      <PagedExplorerBranch
        query={repositories}
        empty={t('factory.noRepositories')}
        labels={labels(t('factory.repository'))}
      >
        {() =>
          selection.repositoryItems.length ? (
            <SelectMenu
              ariaLabel={t('factory.repository')}
              value={selection.repository?.id ?? ''}
              onValueChange={selection.setRepositoryId}
              options={selection.repositoryItems.map((item) => ({
                value: item.id,
                label: `${item.name} · v${item.version}`,
              }))}
            />
          ) : (
            <p>{t('factory.noRepositories')}</p>
          )
        }
      </PagedExplorerBranch>
      {details.data ? (
        <QueryBackgroundNotice
          label={t('factory.repository')}
          error={details.error}
          fetching={details.isFetching}
          onRetry={details.refetch}
        />
      ) : details.error ? (
        <QueryFailureNotice
          title={t('factory.repository')}
          error={details.error}
          onRetry={details.refetch}
        />
      ) : selection.repository ? (
        <QueryLoadingNotice label={t('factory.repository')} />
      ) : null}
      <SelectMenu
        ariaLabel={t('manualBuild.source')}
        value={selection.selectedSource ?? ''}
        onValueChange={selection.setSourceSelection}
        options={selection.sourceOptions}
      />
      {selection.selectedSource === 'exact' ? (
        <label>
          {t('manualBuild.exactRevision')}
          <input
            value={selection.revision}
            onChange={(event) => selection.setRevision(event.currentTarget.value)}
            autoComplete="off"
          />
        </label>
      ) : null}
    </>
  );
}
