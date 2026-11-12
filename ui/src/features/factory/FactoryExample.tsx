import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import { catalogLabels } from './catalogLabels';
import { commandBuild, commandOptions } from './CommandStep';
import { emptyFactoryPermissions, emptyFlowDraft } from './emptyFlowDraft';
import { ExecutorPicker } from './ExecutorPicker';
import { FlowIncomingCompletion } from './FlowIncomingCompletion';
import { publicationKey } from './PublicationStep';
import { mergePermissions } from './flowRequirements';
import { factoryRolesApi } from './roleApi';
import type { FactoryApi, FactoryFlowDetails } from './api';

type S = components['schemas'];
type Draft = S['FactoryFlowDefinitionDraft'];
type Preset = S['FactoryPresetSummary'];
type Editor = NonNullable<Preset['editor']>;
type Command = S['FactoryAuthoringCapabilities']['commands'][number];
type Props = {
  api: FactoryApi;
  projectId: string;
  flowId: string;
  flows: readonly FactoryFlowDetails[];
  onOpen: (draft: Draft) => void;
};

/** Opens a reviewed starting point without saving or changing an existing draft. */
export function FactoryExample(props: Props) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<string>();
  const query = useCursorPage(['factory', 'examples', props.projectId], async (cursor, signal) => {
    const page = await props.api.listPresets(props.projectId, cursor, signal);
    return {
      ...page,
      items: page.items.map((item) => ({ ...item, id: sourceReferenceKey(item.reference) })),
    };
  });
  return (
    <PagedExplorerBranch
      query={query}
      empty={t('factory.noExamples')}
      labels={catalogLabels(t('factory.examples'))}
    >
      {(items) => {
        const choices = items.filter((item) => item.editor);
        const preset = choices.find((item) => item.id === selected);
        return (
          <>
            {!choices.length ? <p>{t('factory.noExamples')}</p> : null}
            {choices.map((item) => (
              <button
                key={item.id}
                type="button"
                aria-pressed={item.id === selected}
                onClick={() => setSelected(item.id)}
              >
                {item.editor?.name} · {item.reference.version}
              </button>
            ))}
            {preset?.editor ? (
              <ExampleBindings key={preset.id} {...props} preset={preset} editor={preset.editor} />
            ) : null}
          </>
        );
      }}
    </PagedExplorerBranch>
  );
}

function ExampleBindings({
  api,
  projectId,
  flowId,
  flows,
  onOpen,
  preset,
  editor,
}: Props & { preset: Preset; editor: Editor }) {
  const { t } = usePresentation();
  const [inputs, setInputs] = useState(emptyFlowDraft);
  const [profiles, setProfiles] = useState<
    Record<string, { profile: S['FactoryRoleProfile']; build: S['FactoryFlowBuildBinding'] }>
  >({});
  const [commands, setCommands] = useState<Record<string, string>>({});
  const [target, setTarget] = useState('');
  const [issues, setIssues] = useState<S['FactoryPresetResolution']['diagnostics']>([]);
  const [error, setError] = useState<Error>();
  const [busy, setBusy] = useState(false);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), []);
  const needs = (kind: string) =>
    preset.parameters.some((item) => item.required && item.kind === kind);
  const commandSteps = editor.steps.filter((step) => step.kind === 'build_command');
  const reasoningSteps = editor.steps.filter((step) => step.kind === 'reasoning');
  const capabilities = useFactoryAuthoringCapabilities(
    { api, projectId },
    commandSteps.length > 0 || needs('publication'),
  );
  const publications = (capabilities.data?.candidate_publications ?? []).filter(
    (choice) => choice.review_url,
  );
  const publisher = publications.find((choice) => publicationKey(choice) === target);
  const choices = (step: Editor['steps'][number]) =>
    (capabilities.data?.commands ?? []).filter((command) => commandSchema(step, command));
  const commandSelections = commandSteps.flatMap((step) => {
    const command = choices(step).find((item) => JSON.stringify(item) === commands[step.alias]);
    const result = command && commandSchema(step, command);
    return command && result
      ? [
          [
            step.alias,
            {
              build: commandBuild(command, step.outcome),
              result,
              permissions: command.permissions ?? emptyFactoryPermissions(),
            },
          ] as const,
        ]
      : [];
  });
  const commandRequired = (step: Editor['steps'][number]) =>
    preset.parameters.some((item) => item.path === `/commands/${step.alias}` && item.required);
  const unsupportedInput = needs('issue_snapshot');
  const ready =
    !unsupportedInput &&
    (!needs('accepted_predecessor') || !!inputs.accepted_predecessor?.outcomes.length) &&
    (!needs('publication') || !!publisher) &&
    commandSteps.every(
      (step) => !commandRequired(step) || commandSelections.some(([alias]) => alias === step.alias),
    ) &&
    reasoningSteps.every((step) => !!profiles[step.alias]);

  async function open() {
    if (!ready || busy) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setBusy(true);
    setError(undefined);
    setIssues([]);
    const publication = publisher && {
      plugin: publisher.plugin,
      target: publisher.target,
      permissions: publisher.permissions,
    };
    const budget = { ...editor.budget };
    const permissions = [
      ...Object.values(profiles).map(({ profile }) => {
        for (const dimension of Object.keys(budget) as (keyof typeof budget)[]) {
          budget[dimension] = Math.max(budget[dimension], profile.definition.budget[dimension]);
        }
        return profile.definition.permissions;
      }),
      ...commandSelections.map(([, selection]) => selection.permissions),
      ...(publication ? [publication.permissions] : []),
    ].reduce(mergePermissions, emptyFactoryPermissions());
    try {
      const result = await api.resolvePreset(
        projectId,
        {
          preset: preset.reference,
          flow_id: flowId,
          flow_only: true,
          execution: { budget, max_active_nodes: editor.max_active_nodes, permissions },
          profiles: Object.fromEntries(
            Object.entries(profiles).map(([alias, { profile, build }]) => [
              alias,
              { role: profile.role, profile: profile.reference, build },
            ]),
          ),
          commands: Object.fromEntries(commandSelections),
          ...(publication ? { publication } : {}),
          ...(inputs.accepted_predecessor
            ? { accepted_predecessor: inputs.accepted_predecessor }
            : {}),
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      if (result.diagnostics.length) setIssues(result.diagnostics);
      else if (result.flow && !result.dependencies.length) onOpen(result.flow);
      else throw new Error(t('factory.exampleUnavailable'));
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure instanceof Error ? failure : new Error());
    } finally {
      if (!controller.signal.aborted) setBusy(false);
    }
  }

  return (
    <fieldset disabled={busy}>
      <legend>{editor.name}</legend>
      <p>{t('factory.exampleHelp')}</p>
      {needs('accepted_predecessor') ? (
        <FlowIncomingCompletion
          value={inputs}
          flows={flows}
          flowApi={api}
          onChange={setInputs}
          empty={t('factory.exampleNeedsPredecessor')}
        />
      ) : null}
      {unsupportedInput ? <p role="status">{t('factory.exampleInputUnsupported')}</p> : null}
      {reasoningSteps.map((step) => (
        <details key={step.alias}>
          <summary>
            {stepLabel(step.alias)}
            {profiles[step.alias] && ` · ${profiles[step.alias]?.profile.definition.name}`}
          </summary>
          <ExecutorPicker
            node={{
              ...(step.result ? { input_schema: step.result.reference } : {}),
              outcomes: step.result
                ? [{ key: step.outcome, kind: 'success', schema: step.result.reference }]
                : [],
            }}
            services={{ api: factoryRolesApi, projectId }}
            onResolved={(profile, build) =>
              setProfiles((current) => ({ ...current, [step.alias]: { profile, build } }))
            }
          />
        </details>
      ))}
      {commandSteps.map((step) => {
        const options = choices(step);
        return (
          <fieldset key={step.alias}>
            <legend>{stepLabel(step.alias)}</legend>
            {!commandRequired(step) ? <p>{t('factory.optionalExampleCommand')}</p> : null}
            {options.length ? (
              <SelectMenu
                ariaLabel={stepLabel(step.alias)}
                value={commands[step.alias] ?? ''}
                options={[
                  { value: '', label: t('factory.chooseInput') },
                  ...commandOptions(options),
                ]}
                onValueChange={(value) =>
                  setCommands((current) => ({ ...current, [step.alias]: value }))
                }
              />
            ) : commandRequired(step) ? (
              <p>{t('factory.noInstalledCommand')}</p>
            ) : null}
          </fieldset>
        );
      })}
      {needs('publication') ? (
        publications.length ? (
          <SelectMenu
            ariaLabel={t('factory.publicationTarget')}
            value={target}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...publications.map((choice) => ({
                value: publicationKey(choice),
                label: `${choice.target.identity} · ${choice.target.version}`,
              })),
            ]}
            onValueChange={setTarget}
          />
        ) : (
          <p>{t('factory.noPublicationTargets')}</p>
        )
      ) : null}
      {capabilities.error ? (
        <QueryFailureNotice
          error={capabilities.error}
          title={t('factory.examples')}
          onRetry={() => void capabilities.refetch()}
        />
      ) : null}
      {issues.length ? (
        <ul role="alert">
          {issues.map(({ path, reason }) => (
            <li key={path}>
              {stepLabel(path.split('/').filter(Boolean).join(' · '))}:{' '}
              {t(reason === 'required' ? 'factory.exampleRequired' : 'factory.exampleUnavailable')}
            </li>
          ))}
        </ul>
      ) : null}
      {error ? (
        <QueryFailureNotice
          error={error}
          title={t('factory.examples')}
          onRetry={() => void open()}
        />
      ) : null}
      <button type="button" disabled={!ready || busy} onClick={() => void open()}>
        {t('factory.openExample')}
      </button>
    </fieldset>
  );
}

function stepLabel(value: string) {
  const label = value.replace(/[._-]/g, ' ');
  return label.charAt(0).toUpperCase() + label.slice(1);
}

function commandSchema(step: Editor['steps'][number], command: Command) {
  if (command.execution.result_output_kind !== 'report') return undefined;
  if (step.result) {
    return command.output_schema &&
      sourceReferenceKey(command.output_schema) === sourceReferenceKey(step.result.reference)
      ? step.result
      : undefined;
  }
  return command.output_definition;
}
