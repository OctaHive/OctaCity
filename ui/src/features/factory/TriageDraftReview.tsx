import { TriageReadiness } from './TriageReadiness';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { issuesPath, issueWorkflowsPath } from '../../app/routes';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import type { FactoryApi, FactoryConfigurationDetails, FactoryFlowDetails } from './api';
import { catalogLabels } from './catalogLabels';
import { ConfigurationEditor } from './ConfigurationEditor';
import { FactoryPipelineInputs } from './FactoryPipelineInputs';
import { FactoryPublication } from './FactoryPublication';
import type { FactoryWorkSources } from './FactoryWorkForm';
import { FlowGraphEditor } from './FlowGraphEditor';
import type { FactoryRolesApi } from './roleApi';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';
import { useFactoryChoices } from './useFactoryChoices';

type Schema = components['schemas'];
/** Each publication uses its ordinary confirmation. A failed later step keeps completed immutable parents. */
export function TriageDraftReview({
  api,
  projectId,
  flowId,
  configurationId,
  flow,
  configuration,
  dependencies,
  workSources,
  workflows,
  roles,
  repository,
  input,
  ready,
  sources = factorySourcesApi,
}: {
  ready: boolean;
  api: FactoryApi;
  projectId: string;
  flowId: string;
  configurationId: string;
  flow: Schema['FactoryFlowDefinitionDraft'];
  configuration: Schema['FactoryConfigurationDefinition'];
  dependencies: Schema['FactoryPresetResolution']['dependencies'];
  workSources?: FactoryWorkSources | undefined;
  workflows?: IssueWorkflowsApi | undefined;
  repository?: components['schemas']['FactoryReadinessRepository'] | undefined;
  input?: Schema['FactoryPresetInputSelection'] | undefined;
  roles: FactoryRolesApi;
  sources?: FactorySourcesApi;
}) {
  const { t } = usePresentation();
  const [remaining, setRemaining] = useState(dependencies);
  const [rootDraft, setRootDraft] = useState(flow);
  const [publishedFlow, setPublishedFlow] = useState<FactoryFlowDetails | null>(null);
  const [configurationDraft, setConfigurationDraft] = useState(configuration);
  const [published, setPublished] = useState<FactoryConfigurationDetails | null>(null);
  const choices = useFactoryChoices(api, projectId);
  const flows = useCursorPage(queryKeys.factoryFlows(projectId), async (cursor, signal) => {
    const page = await api.listFlows(projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((item) => ({ ...item, id: item.flow.closure.root.id })),
    };
  });
  const visibleFlows = flows.data?.pages.flatMap((page) => page.items) ?? [];
  const publicationApi: FactoryApi = {
    ...api,
    async createFlow(request, headers) {
      const result = await api.createFlow(request, headers);
      if (remaining.length) setRemaining((current) => current.slice(1));
      else {
        setPublishedFlow(result.resource);
        setConfigurationDraft((current) => ({ ...current, flow: result.resource.flow }));
      }
      return result;
    },
    async createConfiguration(request, headers) {
      const result = await api.createConfiguration(request, headers);
      setPublished(result.resource);
      return result;
    },
  };
  const child = remaining[0];
  return (
    <section aria-label={t('triage.publication')}>
      <h3>{t('triage.publication')}</h3>
      <p>{t('factory.memoryDraft')}</p>
      <Link to={issueWorkflowsPath(projectId)} target="_blank" rel="noreferrer">
        {t('triage.workflow')}
      </Link>
      <Link to={issuesPath(projectId)} target="_blank" rel="noreferrer">
        {t('issueImport.open')}
      </Link>
      <p>{t('triage.independentIntake')}</p>
      <PagedExplorerBranch
        query={choices}
        empty={t('factory.noChoices')}
        labels={catalogLabels(t('factory.choices'))}
      >
        {() => null}
      </PagedExplorerBranch>
      <PagedExplorerBranch
        query={flows}
        empty={t('factory.noFlows')}
        labels={catalogLabels(t('factory.flows'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {published ? (
        <>
          <TriageReadiness
            api={api}
            configuration={published.configuration.reference}
            repository={repository}
          />
          <FactoryPipelineInputs
            configuration={published}
            api={sources}
            repositories={workSources}
            initialInput={
              input && repository
                ? {
                    id: input.id,
                    expected_version: null,
                    definition: {
                      target: published.configuration.reference,
                      repository,
                      source: input.source,
                      reporter: input.reporter ?? null,
                      connection: input.connection,
                      reporting: input.reporting ?? null,
                      predicate: input.predicate ?? null,
                      enabled: false,
                    },
                  }
                : undefined
            }
          />
        </>
      ) : publishedFlow ? (
        <>
          <p>{t('triage.flowPublished')}</p>
          <ConfigurationEditor
            value={configurationDraft}
            choices={choices.data?.pages.flatMap((page) => page.items) ?? []}
            flows={[publishedFlow]}
            onChange={setConfigurationDraft}
          />
          <FactoryPublication
            key="configuration"
            api={publicationApi}
            ready={ready}
            subject={{
              kind: 'configuration',
              request: {
                id: configurationId,
                project_id: projectId,
                definition: configurationDraft,
              },
            }}
          />
        </>
      ) : (
        <>
          {child ? (
            <p>{t('triage.publishDependencies', { count: remaining.length })}</p>
          ) : (
            <FlowGraphEditor
              value={rootDraft}
              onChange={setRootDraft}
              choices={choices.data?.pages.flatMap((page) => page.items) ?? []}
              subflows={visibleFlows}
              roles={{ api: roles, projectId }}
              sources={{ api: sources, projectId }}
              issueStatus={workflows ? { api: workflows, projectId } : undefined}
            />
          )}
          <FactoryPublication
            key={child?.id ?? flowId}
            api={publicationApi}
            ready={ready}
            subject={{
              kind: 'flow',
              request: {
                id: child?.id ?? flowId,
                project_id: projectId,
                definition: child?.definition ?? rootDraft,
              },
            }}
          />
        </>
      )}
    </section>
  );
}
