import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { factoryResourceId } from './filters';
import { useState } from 'react';
import { ConfigurationPolicyOverview } from './ConfigurationPolicyBinding';
import { FactoryAuthoring } from './FactoryAuthoring';
import { FactoryWorkForm, type FactoryWorkSources } from './FactoryWorkForm';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import type { FactoryApi, FactoryConfiguration, FactoryFlowDetails } from './api';
import { FactoryPipelineInputs } from './FactoryPipelineInputs';
import { FactoryPools } from './FactoryPools';
import { ManagementApiError } from '../../api/client';
import { useFactoryChoices } from './useFactoryChoices';
import styles from './Factory.module.css';
import { aiSettingsPath } from '../../app/routes';
import { flowLabel } from './flowLabels';
import { BUDGET_LABELS } from './ResourceEditors';
import { formatBytes } from '../../shared/display';

/** Published identities are selected from visible bounded pages; edits use separate drafts. */
export function FactoryDefinitions({
  api,
  workflows,
  projectId,
  workSources,
}: {
  api: FactoryApi;
  workflows?: IssueWorkflowsApi | undefined;
  projectId: string;
  workSources?: FactoryWorkSources | undefined;
}) {
  const { t } = usePresentation();
  const [authoring, setAuthoring] = useState<{
    kind: 'flow' | 'configuration' | 'work';
    id: string | null;
    version: number | null;
  } | null>(null);
  const [params, setParams] = useSearchParams();
  const choices = useFactoryChoices(api, projectId);
  const canAuthor = choices.data !== undefined && choices.error === null;
  const configurations = useCursorPage(
    queryKeys.factoryConfigurations(projectId),
    (cursor, signal) => api.listConfigurations(projectId, cursor, signal),
  );
  const flows = useCursorPage(queryKeys.factoryFlows(projectId), async (cursor, signal) => {
    const page = await api.listFlows(projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((item) => ({ ...item, id: item.flow.closure.root.id })),
    };
  });
  const hasPublishedFlow =
    flows.data?.pages.some((page) => page.items.length > 0) && !flows.isError;
  const selectedConfiguration = factoryResourceId(params.get('configuration_id'));
  const selectedFlow = factoryResourceId(params.get('flow_definition_id'));
  const currentConfiguration = useQuery({
    queryKey: queryKeys.factoryCurrent('configuration', selectedConfiguration ?? ''),
    queryFn: ({ signal }) => {
      if (!selectedConfiguration) throw new Error('Configuration selection is missing');
      return api.currentConfiguration(selectedConfiguration, signal);
    },
    enabled:
      selectedConfiguration !== null &&
      configurations.data !== undefined &&
      !configurations.data.pages.some((page) =>
        page.items.some((item) => item.id === selectedConfiguration),
      ),
    refetchOnMount: false,
  });
  const currentFlow = useQuery({
    queryKey: queryKeys.factoryCurrent('flow', selectedFlow ?? ''),
    queryFn: ({ signal }) => {
      if (!selectedFlow) throw new Error('Flow selection is missing');
      return api.currentFlow(selectedFlow, signal);
    },
    enabled:
      selectedFlow !== null &&
      flows.data !== undefined &&
      !flows.data.pages.some((page) => page.items.some((item) => item.id === selectedFlow)),
    refetchOnMount: false,
  });
  const revealedConfiguration =
    currentConfiguration.data?.configuration.reference.project_id === projectId
      ? {
          ...currentConfiguration.data.configuration.reference,
          enabled: currentConfiguration.data.configuration.enabled,
          created_at_unix_ms: currentConfiguration.data.published_at_unix_ms,
          published_at_unix_ms: currentConfiguration.data.published_at_unix_ms,
        }
      : null;
  const revealedFlow =
    currentFlow.data?.project_id === projectId
      ? { ...currentFlow.data, id: currentFlow.data.flow.closure.root.id }
      : null;
  const select = (kind: 'configuration' | 'flow', id: string, version: number) => {
    setAuthoring(null);
    const next = new URLSearchParams(params);
    next.set(kind === 'configuration' ? 'configuration_id' : 'flow_definition_id', id);
    next.set(`${kind}_version`, String(version));
    setParams(next);
  };
  return (
    <section className={styles.definitions}>
      <Link to={aiSettingsPath(projectId)}>{t('factory.agents')}</Link>
      {choices.isPending ? <QueryLoadingNotice label={t('factory.choices')} /> : null}
      {choices.error !== null ? (
        <QueryFailureNotice
          title={
            choices.error instanceof ManagementApiError &&
            choices.error.code === 'capability_unavailable'
              ? t('factory.authoringUnavailable')
              : t('factory.choices')
          }
          error={choices.error}
          onRetry={choices.refetch}
        />
      ) : null}
      <button
        type="button"
        disabled={!canAuthor || authoring !== null || !hasPublishedFlow}
        onClick={() => setAuthoring({ kind: 'configuration', id: null, version: null })}
      >
        {t('factory.createConfiguration')}
      </button>
      {flows.data && !hasPublishedFlow && !flows.isError ? (
        <p>{t('factory.pipelineNeedsGraph')}</p>
      ) : null}
      <button
        type="button"
        disabled={!canAuthor || authoring !== null}
        onClick={() => setAuthoring({ kind: 'flow', id: null, version: null })}
      >
        {t('factory.addPipeline')}
      </button>
      <button
        type="button"
        disabled={!canAuthor || authoring !== null || workSources === undefined}
        onClick={() => setAuthoring({ kind: 'work', id: null, version: null })}
      >
        {t('factory.admitWork')}
      </button>
      {authoring?.kind === 'work' && workSources ? (
        <FactoryWorkForm
          api={api}
          sources={workSources}
          projectId={projectId}
          onDiscard={() => setAuthoring(null)}
        />
      ) : authoring?.kind !== 'work' && authoring?.id === null ? (
        <FactoryAuthoring
          repositories={workSources}
          workflows={workflows}
          key={authoring.kind}
          api={api}
          projectId={projectId}
          kind={authoring.kind}
          onDiscard={() => setAuthoring(null)}
        />
      ) : null}
      <h2>{t('factory.configurations')}</h2>
      {currentConfiguration.isError ? (
        <QueryFailureNotice
          title={t('factory.configurations')}
          error={currentConfiguration.error}
          onRetry={currentConfiguration.refetch}
        />
      ) : null}
      <PagedExplorerBranch
        query={configurations}
        revealed={revealedConfiguration}
        labels={labels(t('factory.configurations'), t('factory.moreConfigurations'))}
        empty={t('factory.unconfigured')}
      >
        {(items) => (
          <ul className={styles.definitionList}>
            {items.map((item) => (
              <li key={item.id} className={styles.definitionItem}>
                <button
                  type="button"
                  onClick={() => select('configuration', item.id, item.version)}
                >
                  {t('factory.inspectConfiguration', { version: item.version })}
                </button>
                {selectedConfiguration === item.id ? (
                  <ConfigurationDetails
                    workflows={workflows}
                    key={`${item.id}:${params.get('configuration_version')}`}
                    api={api}
                    summary={item}
                    repositories={workSources}
                    canAuthor={canAuthor}
                    onManual={
                      workSources
                        ? () => setAuthoring({ kind: 'work', id: null, version: null })
                        : undefined
                    }
                    editing={
                      authoring?.kind === 'configuration' &&
                      authoring.id === item.id &&
                      authoring.version ===
                        readVersion(params.get('configuration_version'), item.version)
                    }
                    onEdit={() =>
                      setAuthoring({
                        kind: 'configuration',
                        id: item.id,
                        version: readVersion(params.get('configuration_version'), item.version),
                      })
                    }
                    onDiscard={() => setAuthoring(null)}
                    version={readVersion(params.get('configuration_version'), item.version)}
                    changeVersion={(selected) => select('configuration', item.id, selected)}
                  />
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      <h2>{t('factory.flows')}</h2>
      {currentFlow.isError ? (
        <QueryFailureNotice
          title={t('factory.flows')}
          error={currentFlow.error}
          onRetry={currentFlow.refetch}
        />
      ) : null}
      <PagedExplorerBranch
        query={flows}
        revealed={revealedFlow}
        labels={labels(t('factory.flows'), t('factory.moreFlows'))}
        empty={t('factory.noFlows')}
      >
        {(items) => (
          <ul className={styles.definitionList}>
            {items.map((item) => (
              <li key={item.id} className={styles.definitionItem}>
                <button
                  type="button"
                  onClick={() => select('flow', item.id, item.flow.closure.root.version)}
                >
                  {flowLabel(item.flow, t('factory.immutableFlow'))}
                </button>
                {selectedFlow === item.id ? (
                  <FlowDetails
                    repositories={workSources}
                    workflows={workflows}
                    key={`${item.id}:${params.get('flow_version')}`}
                    api={api}
                    summary={item}
                    canAuthor={canAuthor}
                    editing={
                      authoring?.kind === 'flow' &&
                      authoring.id === item.id &&
                      authoring.version ===
                        readVersion(params.get('flow_version'), item.flow.closure.root.version)
                    }
                    onEdit={() =>
                      setAuthoring({
                        kind: 'flow',
                        id: item.id,
                        version: readVersion(
                          params.get('flow_version'),
                          item.flow.closure.root.version,
                        ),
                      })
                    }
                    onDiscard={() => setAuthoring(null)}
                    version={readVersion(
                      params.get('flow_version'),
                      item.flow.closure.root.version,
                    )}
                    changeVersion={(selected) => select('flow', item.id, selected)}
                  />
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
    </section>
  );
}

function ConfigurationDetails({
  repositories,
  onManual,
  api,
  workflows,
  summary,
  version,
  changeVersion,
  editing,
  onEdit,
  onDiscard,
  canAuthor,
}: {
  onManual?: (() => void) | undefined;
  repositories?: FactoryWorkSources | undefined;
  api: FactoryApi;
  workflows?: IssueWorkflowsApi | undefined;
  summary: FactoryConfiguration;
  version: number;
  changeVersion: (version: number) => void;
  editing: boolean;
  onEdit: () => void;
  onDiscard: () => void;
  canAuthor: boolean;
}) {
  const { t, locale } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.factoryConfiguration(summary.id, version),
    queryFn: ({ signal }) => api.getConfiguration(summary.id, version, signal),
  });
  if (query.isPending) return <QueryLoadingNotice label={t('factory.configurations')} />;
  if (query.data === undefined)
    return (
      <QueryFailureNotice
        title={t('factory.configurations')}
        error={query.error}
        onRetry={query.refetch}
      />
    );
  const configuration = query.data.configuration;
  return (
    <section
      className={styles.definitionDetails}
      aria-label={t('factory.configurationVersion', { version })}
    >
      <header className={styles.definitionToolbar}>
        <h3>{t('factory.configurationVersion', { version })}</h3>
        <VersionPicker current={summary.version} selected={version} onChange={changeVersion} />
      </header>
      <dl className={styles.definitionFacts}>
        <div>
          <dt>{t('factory.rootFlow')}</dt>
          <dd>{flowLabel(configuration.flow, t('factory.immutableFlow'))}</dd>
        </div>
        <div>
          <dt>{t('factory.executionIntake')}</dt>
          <dd>
            {t(configuration.enabled ? 'factory.executionEnabled' : 'factory.executionDisabled')}
          </dd>
        </div>
      </dl>
      <section>
        <h4>{t('factory.hardBudget')}</h4>
        <dl className={styles.definitionFacts}>
          {(Object.keys(BUDGET_LABELS) as (keyof typeof configuration.hard_budget)[]).map((key) => (
            <div key={key}>
              <dt>
                {t(
                  key === 'max_elapsed_millis'
                    ? 'factory.executionTime'
                    : key === 'max_output_bytes'
                      ? 'factory.resultSizeLimit'
                      : BUDGET_LABELS[key],
                )}
              </dt>
              <dd>
                {key === 'max_output_bytes'
                  ? formatBytes(configuration.hard_budget[key], locale)
                  : key === 'max_elapsed_millis'
                    ? t('factory.elapsedSeconds', {
                        value: (configuration.hard_budget[key] / 1000).toLocaleString(locale),
                      })
                    : configuration.hard_budget[key].toLocaleString(locale)}
              </dd>
            </div>
          ))}
        </dl>
      </section>
      <ConfigurationPolicyOverview
        project={configuration.reference.project_id}
        kind="launch"
        binding={configuration.admission_policy}
      />
      <ConfigurationPolicyOverview
        project={configuration.reference.project_id}
        kind="access"
        binding={configuration.permission_ceiling}
      />
      <details>
        <summary>{t('factory.definitionDetails')}</summary>
        <dl className={styles.definitionFacts}>
          <div>
            <dt>{t('factory.admissionPolicy')}</dt>
            <dd>
              <span>
                {'project_policy' in configuration.admission_policy
                  ? `${configuration.admission_policy.project_policy.id} · ${configuration.admission_policy.project_policy.version}`
                  : `${configuration.admission_policy.identity} · ${configuration.admission_policy.version}`}
              </span>
              <br />
              {'project_policy' in configuration.admission_policy
                ? configuration.admission_policy.project_policy.digest
                : configuration.admission_policy.digest}
            </dd>
          </div>
          <div>
            <dt>{t('factory.permissionCeiling')}</dt>
            <dd>
              <span>
                {'project_policy' in configuration.permission_ceiling
                  ? `${configuration.permission_ceiling.project_policy.id} · ${configuration.permission_ceiling.project_policy.version}`
                  : `${configuration.permission_ceiling.identity} · ${configuration.permission_ceiling.version}`}
              </span>
              <br />
              {'project_policy' in configuration.permission_ceiling
                ? configuration.permission_ceiling.project_policy.digest
                : configuration.permission_ceiling.digest}
            </dd>
          </div>
          {Object.entries(configuration.hard_budget).map(([key, amount]) => (
            <div key={key}>
              <dt>{key}</dt>
              <dd>{amount}</dd>
            </div>
          ))}
        </dl>
      </details>
      {editing ? (
        <FactoryAuthoring
          repositories={repositories}
          workflows={workflows}
          api={api}
          baseline={query.data}
          currentVersion={summary.version}
          onDiscard={onDiscard}
        />
      ) : (
        <div className={styles.definitionActions}>
          <button type="button" disabled={!canAuthor} onClick={onEdit}>
            {t('factory.editConfiguration')}
          </button>
        </div>
      )}
      <FactoryPipelineInputs
        configuration={query.data}
        onManual={onManual}
        repositories={repositories}
      />
      <FactoryPools api={api} configuration={query.data} />
    </section>
  );
}

function FlowDetails({
  repositories,
  api,
  workflows,
  summary,
  version,
  changeVersion,
  editing,
  onEdit,
  onDiscard,
  canAuthor,
}: {
  repositories?: FactoryWorkSources | undefined;
  api: FactoryApi;
  workflows?: IssueWorkflowsApi | undefined;
  summary: FactoryFlowDetails;
  version: number;
  changeVersion: (version: number) => void;
  editing: boolean;
  onEdit: () => void;
  onDiscard: () => void;
  canAuthor: boolean;
}) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.factoryFlow(summary.flow.closure.root.id, version),
    queryFn: ({ signal }) => api.getFlow(summary.flow.closure.root.id, version, signal),
  });
  if (query.isPending) return <QueryLoadingNotice label={t('factory.flows')} />;
  if (query.data === undefined)
    return (
      <QueryFailureNotice title={t('factory.flows')} error={query.error} onRetry={query.refetch} />
    );
  return (
    <section className={styles.definitionDetails}>
      <header className={styles.definitionToolbar}>
        <h3>
          {query.data.flow.closure.definitions.find(
            (definition) =>
              definition.reference.id === query.data.flow.closure.root.id &&
              definition.reference.version === query.data.flow.closure.root.version,
          )?.name ?? t('factory.immutableFlow')}
        </h3>
        <VersionPicker
          current={summary.flow.closure.root.version}
          selected={version}
          onChange={changeVersion}
        />
      </header>
      {editing ? (
        <FactoryAuthoring
          repositories={repositories}
          workflows={workflows}
          api={api}
          baseline={query.data}
          currentVersion={summary.flow.closure.root.version}
          onDiscard={onDiscard}
        />
      ) : (
        <button type="button" disabled={!canAuthor} onClick={onEdit}>
          {t('factory.editFlow')}
        </button>
      )}
      {query.data.flow.closure.definitions.map((definition) => (
        <section key={`${definition.reference.id}:${definition.reference.version}`}>
          <h4>{definition.entry}</h4>
          <ul>
            {definition.nodes.map((node) => (
              <li key={node.key}>
                {node.key} · {node.kind}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </section>
  );
}

function VersionPicker({
  current,
  selected,
  onChange,
}: {
  current: number;
  selected: number;
  onChange: (version: number) => void;
}) {
  const { t } = usePresentation();
  const [search, setSearch] = useState('');
  const requested = Number(search);
  const head = !search.trim()
    ? current
    : Number.isSafeInteger(requested) && requested > 0 && requested <= current
      ? requested
      : 0;
  const values = Array.from({ length: Math.min(head, 100) }, (_, index) => head - index);
  if (!values.includes(selected)) values.push(selected);
  return (
    <div className={styles.versionPicker}>
      <span>{t('factory.version')}</span>
      <SelectMenu
        search={
          current > 100
            ? { label: t('factory.findVersion'), value: search, onChange: setSearch }
            : undefined
        }
        ariaLabel={t('factory.version')}
        value={String(selected)}
        onValueChange={(value) => onChange(Number(value))}
        options={values.map((value) => ({
          value: String(value),
          label: t('factory.versionLabel', { version: value }),
        }))}
      />
    </div>
  );
}

function readVersion(raw: string | null, fallback: number) {
  const value = raw === null ? fallback : Number(raw);
  return Number.isSafeInteger(value) && value > 0 ? value : fallback;
}
function labels(label: string, loadMore: string) {
  return { failure: label, loading: label, stale: label, loadMore, loadingMore: label };
}
