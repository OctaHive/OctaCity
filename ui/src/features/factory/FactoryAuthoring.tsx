import { FactoryPublication } from './FactoryPublication';
import { FactoryExample } from './FactoryExample';
import { emptyFlowDraft } from './emptyFlowDraft';
import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import type { FactoryApi, FactoryConfigurationDetails, FactoryFlowDetails } from './api';
import { NewConfigurationDraft, ExistingConfigurationDraft } from './ConfigurationDraft';
import { SchemaEditor } from './SchemaEditor';
import { factoryFieldId } from './fieldFocus';
import { FlowGraphEditor } from './FlowGraphEditor';
import { useFactoryChoices } from './useFactoryChoices';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { factoryRolesApi } from './roleApi';
import { factorySourcesApi } from './sourceApi';
import { projectDocumentsApi } from '../../api/projectDocuments';
import styles from './Factory.module.css';

import type { SourceRepositories } from './SourceInputEditor';

/** Hosts Flow authoring and mounts an independent Configuration draft for the selected baseline. */
export function FactoryAuthoring({
  api,
  repositories,
  workflows,
  baseline,
  onDiscard,
  projectId: newProjectId,
  kind,
  currentVersion,
}: {
  api: FactoryApi;
  repositories?: SourceRepositories | undefined;
  workflows?: IssueWorkflowsApi | undefined;
  baseline?: FactoryConfigurationDetails | FactoryFlowDetails;
  projectId?: string;
  kind?: 'flow' | 'configuration';
  currentVersion?: number;
  onDiscard: () => void;
}) {
  const { t } = usePresentation();
  const configuration = baseline && 'configuration' in baseline ? baseline.configuration : null;
  const [newId] = useState(() => globalThis.crypto.randomUUID());
  const [advancedContracts, setAdvancedContracts] = useState(false);
  const [processEditing, setProcessEditing] = useState(false);
  const [stepEditing, setStepEditing] = useState(false);
  const [examplesOpen, setExamplesOpen] = useState(false);
  const [blank] = useState(() => (kind === 'flow' ? emptyFlowDraft() : null));
  const projectId =
    configuration?.reference.project_id ??
    (baseline && 'project_id' in baseline ? baseline.project_id : (newProjectId ?? ''));
  const flow = configuration?.flow ?? (baseline && 'flow' in baseline ? baseline.flow : null);
  const [draft, setDraft] = useState<components['schemas']['FactoryFlowDefinitionDraft'] | null>(
    () => {
      if (blank) return blank;
      if (configuration || !flow) return null;
      const root = flow.closure.definitions.find(
        (definition) =>
          definition.reference.id === flow.closure.root.id &&
          definition.reference.version === flow.closure.root.version,
      );
      if (!root) return null;
      const { reference: _reference, ...definition } = root;
      void _reference;
      return structuredClone({
        root: definition,
        data_schemas: flow.data_schemas,
        limits: flow.limits,
        ...(flow.pools ? { pools: flow.pools } : {}),
        ...(flow.accepted_predecessor ? { accepted_predecessor: flow.accepted_predecessor } : {}),
      });
    },
  );
  const choices = useFactoryChoices(api, projectId);
  const flows = useCursorPage(queryKeys.factoryFlows(projectId), async (cursor, signal) => {
    const page = await api.listFlows(projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((item) => ({ ...item, id: item.flow.closure.root.id })),
    };
  });
  const visibleFlows = flows.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <section className={styles.editor}>
      {kind === 'flow' ? <p>{t('factory.pipelineAuthoringHelp')}</p> : null}
      <p>{t('factory.memoryDraft')}</p>
      <div className={styles.definitionActions}>
        <button type="button" onClick={onDiscard}>
          {t('factory.discardDraft')}
        </button>
      </div>
      {blank && draft === blank ? (
        <details onToggle={(event) => setExamplesOpen(event.currentTarget.open)}>
          <summary>{t('factory.examples')}</summary>
          {examplesOpen ? (
            <FactoryExample
              api={api}
              projectId={projectId}
              flowId={newId}
              flows={visibleFlows}
              onOpen={setDraft}
            />
          ) : null}
        </details>
      ) : null}
      <PagedExplorerBranch
        query={choices}
        empty={t('factory.noChoices')}
        labels={labels(t('factory.choices'), t('factory.moreChoices'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {choices.data ? (
        configuration ? (
          <ExistingConfigurationDraft
            key={configuration.reference.id}
            baseline={configuration}
            publicationVersion={currentVersion ?? configuration.reference.version}
            flows={visibleFlows}
            api={api}
          />
        ) : kind === 'configuration' && visibleFlows[0] ? (
          <NewConfigurationDraft
            api={api}
            projectId={projectId}
            id={newId}
            initial={{
              enabled: false,
              flow: visibleFlows[0].flow,
              hard_budget: visibleFlows[0].flow.limits.budget,
              wip_limits: {
                max_active_runs: 1,
                max_active_nodes: visibleFlows[0].flow.limits.max_wip,
              },
            }}
            flows={visibleFlows}
          />
        ) : draft ? (
          <>
            <FlowGraphEditor
              codeResources={repositories ? { projectId, repositories } : undefined}
              authoring={{ api, projectId }}
              onStepEditingChange={setStepEditing}
              onProcessEditingChange={setProcessEditing}
              documents={{ api: projectDocumentsApi, projectId }}
              issueStatus={workflows ? { api: workflows, projectId } : undefined}
              roles={{ api: factoryRolesApi, projectId }}
              sources={{ api: factorySourcesApi, projectId }}
              value={draft}
              choices={choices.data.pages.flatMap((page) => page.items)}
              subflows={visibleFlows}
              onChange={setDraft}
            />
            <FactoryPublication
              api={api}
              ready={!processEditing && !stepEditing}
              subject={{
                kind: 'flow',
                request: {
                  id: flow?.closure.root.id ?? newId,
                  project_id: projectId,
                  definition: draft,
                },
              }}
              {...(flow ? { version: currentVersion ?? flow.closure.root.version } : {})}
            />
          </>
        ) : null
      ) : null}
      <PagedExplorerBranch
        query={flows}
        empty={t('factory.noFlows')}
        labels={labels(t('factory.flows'), t('factory.moreFlows'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {draft ? (
        <details onToggle={(event) => setAdvancedContracts(event.currentTarget.open)}>
          <summary id={factoryFieldId('/data_schemas')}>{t('factory.advancedContracts')}</summary>
          {advancedContracts ? (
            <SchemaEditor
              api={api}
              projectId={projectId}
              onAdd={(schema) =>
                setDraft(
                  (current) =>
                    current && {
                      ...current,
                      data_schemas: [
                        ...current.data_schemas.filter(
                          (item) =>
                            item.reference.identity !== schema.reference.identity ||
                            item.reference.version !== schema.reference.version,
                        ),
                        schema,
                      ],
                    },
                )
              }
            />
          ) : null}
        </details>
      ) : null}
    </section>
  );
}
function labels(label: string, loadMore: string) {
  return { loading: label, failure: label, stale: label, loadingMore: label, loadMore };
}
