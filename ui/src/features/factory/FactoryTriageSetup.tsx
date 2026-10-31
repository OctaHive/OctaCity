import { useState } from 'react';
import { Link } from 'react-router-dom';
import { projectDocumentsApi, type ProjectDocumentsApi } from '../../api/projectDocuments';
import { ProjectDocumentPublisher } from '../../shared/ProjectDocumentPublisher';
import type { IssueWorkflowsApi } from '../../api/issueWorkflows';
import { TriageReportingEffects } from './TriageReportingEffects';
import { TriageSourceSelection, type TriageSourceDraft } from './TriageSourceSelection';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';
import { TriageDraftReview } from './TriageDraftReview';
import { TriageExecutionFields, type TriagePolicies } from './TriageExecutionFields';
import { TriageCodeRepository, TriageContextFields } from './TriageContextFields';
import type { FactoryWorkSources } from './FactoryWorkForm';
import { TriageStageProfile } from './TriageStageProfile';
import { factoryRolesApi, type FactoryRolesApi } from './roleApi';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { aiSettingsPath, CONSOLE_PATHS } from '../../app/routes';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { FactoryApi } from './api';
import { catalogLabels } from './catalogLabels';

/** Setup reviews exact release metadata; private review drafts remain in this mounted component. */
export function FactoryTriageSetup({
  api,
  projectId,
  onClose,
  roles = factoryRolesApi,
  workSources,
  workflows,
  sources = factorySourcesApi,
  documents = projectDocumentsApi,
}: {
  api: FactoryApi;
  projectId: string;
  onClose: () => void;
  roles?: FactoryRolesApi;
  workSources?: FactoryWorkSources | undefined;
  workflows?: IssueWorkflowsApi | undefined;
  sources?: FactorySourcesApi;
  documents?: ProjectDocumentsApi;
}) {
  const { t } = usePresentation();
  const presets = useCursorPage(queryKeys.factoryPresets(projectId), async (cursor, signal) => {
    const page = await api.listPresets(projectId, cursor, signal);
    return { ...page, items: page.items.map((item) => ({ ...item, id: item.reference.identity })) };
  });
  const [resolution, setResolution] = useState<
    components['schemas']['FactoryPresetResolution'] | null
  >(null);
  const [profiles, setProfiles] = useState<
    Record<string, components['schemas']['FactoryRoleProfile']>
  >({});
  const [context, setContext] = useState<
    NonNullable<components['schemas']['FactoryPresetResolutionRequest']['context']>
  >({});
  const [repository, setRepository] =
    useState<NonNullable<components['schemas']['FactoryPresetResolutionRequest']['repository']>>();
  const [policies, setPolicies] = useState<TriagePolicies>();
  const [identities] = useState(() => ({
    flow_id: crypto.randomUUID(),
    configuration_id: crypto.randomUUID(),
  }));
  const [input, setInput] = useState<TriageSourceDraft>();
  const [effects, setEffects] = useState<
    NonNullable<components['schemas']['FactoryPresetResolutionRequest']['effects']>
  >({});
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const selections = {
    ...(policies ?? {}),
    ...(input ? { input: input.input } : {}),
    ...(Object.keys(effects).length ? { effects } : {}),
    ...(repository ? { repository } : {}),
    ...(Object.keys(context).length ? { context } : {}),
    ...(Object.keys(profiles).length
      ? {
          profiles: Object.fromEntries(
            Object.entries(profiles).map(([alias, value]) => [
              alias,
              { role: value.role, profile: value.reference },
            ]),
          ),
        }
      : {}),
  };
  const signature = JSON.stringify(selections);
  const [draft, setDraft] = useState<{
    resolution: components['schemas']['FactoryPresetResolution'];
    signature: string;
  } | null>(null);
  const ready =
    draft?.signature === signature && !pending && !error && resolution?.diagnostics.length === 0;
  async function review(
    preset: components['schemas']['FactoryImmutableReferenceResource'],
    complete = false,
  ) {
    setPending(true);
    setError(null);
    try {
      const result = await api.resolvePreset(projectId, {
        preset,
        ...(complete ? identities : {}),
        ...selections,
      });
      setResolution(result);
      if (complete && result.flow && result.configuration)
        setDraft({ resolution: result, signature });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }

  return (
    <section aria-label={t('triage.title')}>
      <h2>{t('triage.title')}</h2>
      <button type="button" onClick={onClose}>
        {t('triage.close')}
      </button>
      <PagedExplorerBranch
        query={presets}
        empty={t('triage.unavailable')}
        labels={catalogLabels(t('triage.title'))}
      >
        {(items) =>
          items
            .filter((item) => item.reference.identity === 'triage')
            .map((item) => (
              <button
                key={item.id}
                type="button"
                disabled={pending}
                onClick={() => void review(item.reference)}
              >
                {t('triage.reviewPrerequisites')}
              </button>
            ))
        }
      </PagedExplorerBranch>
      {error ? (
        <QueryFailureNotice
          title={t('triage.title')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
      {resolution ? (
        <>
          <h3>{t('triage.completion')}</h3>
          <p>{t('triage.completionSummary')}</p>
          {resolution.completion.kind === 'draft' ? (
            <details>
              <summary>{t('triage.viewCompletion')}</summary>
              <p>{t('triage.completionHelp')}</p>
              <p style={{ whiteSpace: 'pre-wrap' }}>{resolution.completion.content}</p>
            </details>
          ) : null}
          <ProjectDocumentPublisher api={documents} projectId={projectId} />
          {workSources ? (
            <TriageCodeRepository
              projectId={projectId}
              sources={workSources}
              value={repository}
              onChange={setRepository}
            />
          ) : null}
          {presets.data?.pages
            .flatMap((page) => page.items)
            .find((item) => item.reference.digest === resolution.preset.digest)
            ?.profile_aliases.map((alias) => (
              <TriageStageProfile
                key={alias}
                title={
                  alias === 'eligibility'
                    ? t('triage.projectAnalyst')
                    : alias === 'classification'
                      ? t('triage.systemsAnalyst')
                      : alias
                }
                projectId={projectId}
                api={roles}
                value={profiles[alias]}
                onChange={(profile) => setProfiles({ ...profiles, [alias]: profile })}
              >
                <TriageContextFields
                  projectId={projectId}
                  api={documents}
                  value={context[alias]}
                  onChange={(value) => setContext({ ...context, [alias]: value })}
                />
              </TriageStageProfile>
            ))}
          <TriageExecutionFields
            api={api}
            projectId={projectId}
            value={policies}
            onChange={setPolicies}
          />
          <button
            type="button"
            disabled={pending}
            onClick={() => void review(resolution.preset, true)}
          >
            {t('triage.resolve')}
          </button>
          {resolution.diagnostics.length ? (
            <TriageDependencies projectId={projectId} problems={resolution.diagnostics} />
          ) : null}
          {input && resolution.flow ? (
            <TriageReportingEffects
              flow={resolution.flow}
              selection={input}
              value={effects}
              onChange={setEffects}
            />
          ) : null}
          {resolution.configuration && workSources ? (
            <TriageSourceSelection
              api={api}
              sources={sources}
              workSources={workSources}
              request={{
                id: identities.configuration_id,
                project_id: projectId,
                definition: resolution.configuration,
              }}
              onSelect={(value) => {
                setInput(value);
                setRepository(value.repository);
              }}
            />
          ) : null}
          {draft && !ready ? <p role="status">{t('triage.retainedDraft')}</p> : null}
          {draft?.resolution.flow && draft.resolution.configuration ? (
            <TriageDraftReview
              key={JSON.stringify(draft.resolution.configuration)}
              ready={ready}
              api={api}
              projectId={projectId}
              flowId={identities.flow_id}
              configurationId={identities.configuration_id}
              flow={draft.resolution.flow}
              configuration={draft.resolution.configuration}
              dependencies={draft.resolution.dependencies}
              workSources={workSources}
              workflows={workflows}
              roles={roles}
              sources={sources}
              repository={repository}
              input={input?.input}
            />
          ) : null}
        </>
      ) : null}
    </section>
  );
}

/** Localizes only public parameter locations and safe reasons, never raw paths or hidden identities. */
function TriageDependencies({
  projectId,
  problems,
}: {
  projectId: string;
  problems: components['schemas']['FactoryPresetResolution']['diagnostics'];
}) {
  const { t } = usePresentation();
  const subjects: Record<string, MessageKey> = {
    profiles: 'factory.profiles',
    context: 'triage.context',
    repository: 'triage.repository',
    execution: 'triage.execution',
    configuration: 'triage.execution',
    input: 'triage.sourceSelection',
    effects: 'triage.effects',
    flow_id: 'triage.publication',
    configuration_id: 'triage.publication',
  };
  const stages: Record<string, MessageKey> = {
    eligibility: 'triage.projectAnalyst',
    classification: 'triage.systemsAnalyst',
  };
  const reasons = {
    required: 'triage.dependencyRequired',
    unavailable: 'triage.dependencyUnavailable',
  } satisfies Record<
    components['schemas']['FactoryPresetResolution']['diagnostics'][number]['reason'],
    MessageKey
  >;
  return (
    <ul aria-label={t('triage.dependencies')}>
      {problems.map(({ path, reason }) => {
        const [, kind = '', alias = '', detail] = path.split('/');
        const stage = kind === 'profiles' || kind === 'context' ? stages[alias] : undefined;
        const build = kind === 'profiles' && detail === 'build';
        const subject = build
          ? 'triage.buildBinding'
          : kind === 'context' && detail === 'goals'
            ? 'triage.goals'
            : kind === 'context' && detail === 'rules'
              ? 'triage.rules'
              : (subjects[kind] ?? 'triage.dependency');
        return (
          <li key={`${path}:${reason}`}>
            {stage ? `${t(stage)} · ` : ''}
            {t(subject)}: {t(reasons[reason])}{' '}
            {kind === 'profiles' ? (
              <Link
                to={aiSettingsPath(projectId, build ? 'runtimes' : 'agents')}
                target="_blank"
                rel="noreferrer"
              >
                {t(build ? 'factory.configureRuntime' : 'factory.agents')}
              </Link>
            ) : kind === 'input' || kind === 'effects' ? (
              <Link to={CONSOLE_PATHS.integrations} target="_blank" rel="noreferrer">
                {t('factory.manageIntegrations')}
              </Link>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}
