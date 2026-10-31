import { FactoryPublication } from './FactoryPublication';
import { emptyFlowDraft } from './emptyFlowDraft';
import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import type { FactoryApi, FactoryConfigurationDetails, FactoryFlowDetails } from './api';
import { ConfigurationEditor } from './ConfigurationEditor';
import { SchemaEditor } from './SchemaEditor';
import { FlowGraphEditor } from './FlowGraphEditor';
import { useFactoryChoices } from './useFactoryChoices';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { factoryRolesApi } from './roleApi';
import { factorySourcesApi } from './sourceApi';
import { projectDocumentsApi } from '../../api/projectDocuments';

type Choice = components['schemas']['FactoryConfigurationChoiceResource'];
/** Entire editor state is owned here and discarded when its immutable baseline or Project changes. */
export function FactoryAuthoring({
  api,
  workflows,
  baseline,
  onDiscard,
  projectId: newProjectId,
  kind,
  currentVersion,
}: {
  api: FactoryApi;
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
  const [processEditing, setProcessEditing] = useState(false);
  const projectId =
    configuration?.reference.project_id ??
    (baseline && 'project_id' in baseline ? baseline.project_id : (newProjectId ?? ''));
  const flow = configuration?.flow ?? (baseline && 'flow' in baseline ? baseline.flow : null);
  const [draft, setDraft] = useState<components['schemas']['FactoryFlowDefinitionDraft'] | null>(
    () => {
      if (kind === 'flow') return emptyFlowDraft();
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
    <section>
      <p>{t('factory.memoryDraft')}</p>
      <button type="button" onClick={onDiscard}>
        {t('factory.discardDraft')}
      </button>
      {draft ? (
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
      <PagedExplorerBranch
        query={choices}
        empty={t('factory.noChoices')}
        labels={labels(t('factory.choices'), t('factory.moreChoices'))}
      >
        {() => null}
      </PagedExplorerBranch>
      {choices.data ? (
        configuration ? (
          <ConfigurationDraft
            key={configuration.reference.id}
            baseline={configuration}
            publicationVersion={currentVersion ?? configuration.reference.version}
            choices={choices.data.pages.flatMap((page) => page.items)}
            flows={visibleFlows}
            api={api}
          />
        ) : kind === 'configuration' && visibleFlows[0] ? (
          <NewConfiguration
            api={api}
            projectId={projectId}
            id={newId}
            flow={visibleFlows[0]}
            choices={choices.data.pages.flatMap((page) => page.items)}
            flows={visibleFlows}
          />
        ) : draft ? (
          <>
            <FlowGraphEditor
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
              ready={!processEditing}
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
    </section>
  );
}
function ConfigurationDraft({
  baseline,
  publicationVersion,
  choices,
  flows,
  api,
}: {
  api: FactoryApi;
  baseline: FactoryConfigurationDetails['configuration'];
  publicationVersion: number;
  choices: readonly Choice[];
  flows: readonly FactoryFlowDetails[];
}) {
  const [draft, setDraft] = useState<components['schemas']['FactoryConfigurationDefinition']>(
    () => ({
      enabled: baseline.enabled,
      admission_policy: alias(baseline.admission_policy, 'admission_policy', choices),
      permission_ceiling: alias(baseline.permission_ceiling, 'permission_ceiling', choices),
      hard_budget: structuredClone(baseline.hard_budget),
      wip_limits: { ...baseline.wip_limits },
      flow: structuredClone(baseline.flow),
    }),
  );
  return (
    <>
      <ConfigurationEditor value={draft} onChange={setDraft} choices={choices} flows={flows} />
      <FactoryPublication
        api={api}
        subject={{
          kind: 'configuration',
          request: {
            id: baseline.reference.id,
            project_id: baseline.reference.project_id,
            definition: draft,
          },
        }}
        version={publicationVersion}
      />
    </>
  );
}
function alias(
  reference: components['schemas']['FactoryImmutableReferenceResource'],
  kind: Choice['kind'],
  choices: readonly Choice[],
) {
  return (
    choices.find(
      (choice) =>
        choice.kind === kind &&
        choice.reference.kind === 'immutable' &&
        choice.reference.reference.identity === reference.identity &&
        choice.reference.reference.version === reference.version &&
        choice.reference.reference.digest === reference.digest,
    )?.alias ?? ''
  );
}
function labels(label: string, loadMore: string) {
  return { loading: label, failure: label, stale: label, loadingMore: label, loadMore };
}

function NewConfiguration({
  api,
  projectId,
  id,
  flow,
  choices,
  flows,
}: {
  api: FactoryApi;
  projectId: string;
  id: string;
  flow: FactoryFlowDetails;
  choices: readonly Choice[];
  flows: readonly FactoryFlowDetails[];
}) {
  const [draft, setDraft] = useState<components['schemas']['FactoryConfigurationDefinition']>(
    () => ({
      enabled: false,
      admission_policy: '',
      permission_ceiling: '',
      flow: structuredClone(flow.flow),
      hard_budget: structuredClone(flow.flow.limits.budget),
      wip_limits: { max_active_runs: 1, max_active_nodes: flow.flow.limits.max_wip },
    }),
  );
  return (
    <>
      <ConfigurationEditor value={draft} onChange={setDraft} choices={choices} flows={flows} />
      <FactoryPublication
        api={api}
        subject={{
          kind: 'configuration',
          request: { id, project_id: projectId, definition: draft },
        }}
      />
    </>
  );
}
