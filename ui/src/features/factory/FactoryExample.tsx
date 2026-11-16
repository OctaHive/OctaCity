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
import { FillIssueFields, type FieldWrite } from './FillIssueFields';
import { SnapshotFieldSelectors } from './SnapshotFieldSelectors';
import { fieldWriteMatches, issueFieldSourceKey, selectedIssueFields } from './issueFields';
import { IssueAdmission, validIssueAdmission, type IssueAdmissionPolicy } from './IssueAdmission';
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
  const [fieldForms, setFieldForms] = useState<Record<string, FieldWrite>>({});
  const [inputs, setInputs] = useState(emptyFlowDraft);
  const [profiles, setProfiles] = useState<
    Record<string, { profile: S['FactoryRoleProfile']; build: S['FactoryFlowBuildBinding'] }>
  >({});
  const [commands, setCommands] = useState<Record<string, string>>({});
  const [target, setTarget] = useState('');
  const [admission, setAdmission] = useState<IssueAdmissionPolicy>();
  const [thresholds, setThresholds] = useState<Record<string, string>>({});
  const [consumerFields, setConsumerFields] = useState<
    Record<string, S['FactoryFlowIssueFieldSelection']>
  >({});
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
    commandSteps.length > 0 ||
      needs('publication') ||
      needs('issue_snapshot') ||
      Object.keys(editor.issue_fields ?? {}).length > 0,
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
  const catalog = capabilities.data?.issue_fields;
  const configuredForms = Object.entries(editor.issue_fields ?? {});
  const missingForms = configuredForms.filter(([node]) => !Object.hasOwn(fieldForms, node));
  const writer = capabilities.data?.issue_classification;
  // Pin defaults when first displayed, just like an edited form. Refreshes require reconciliation.
  if (catalog && writer && !capabilities.isFetching && !capabilities.error && missingForms.length) {
    setFieldForms({
      ...fieldForms,
      ...Object.fromEntries(
        missingForms.map(([node, fields]) => [
          node,
          {
            writer: writer.plugin,
            policy: writer.policy,
            form: { project_id: projectId, definition_version: catalog.definition_version, fields },
          },
        ]),
      ),
    });
  }
  const selectedForms = Object.fromEntries(
    configuredForms.map(([node]) => [node, fieldForms[node] ?? null]),
  );
  const formsReady =
    !Object.keys(selectedForms).length ||
    (!capabilities.isFetching &&
      !capabilities.error &&
      Object.values(selectedForms).every(
        (form) =>
          form &&
          fieldWriteMatches(form, catalog, capabilities.data?.issue_classification, projectId),
      ));
  const snapshotSelections = Object.fromEntries(
    Object.entries(editor.issue_snapshot_fields ?? {}).map(([alias, source]) => [
      alias,
      consumerFields[alias] ?? { source, required: true },
    ]),
  );
  // Admission conversions belong to source identities, not just the template's local aliases.
  function changeSnapshotFields(fields: Record<string, S['FactoryFlowIssueFieldSelection']>) {
    const changed = new Set(
      Object.entries(fields).flatMap(([alias, selection]) =>
        snapshotSelections[alias] &&
        issueFieldSourceKey(snapshotSelections[alias].source) !==
          issueFieldSourceKey(selection.source)
          ? [alias]
          : [],
      ),
    );
    setConsumerFields(fields);
    if (changed.size)
      setAdmission((policy) =>
        policy
          ? {
              ...policy,
              priority: changed.has(policy.priority.field)
                ? { ...policy.priority, values: {} }
                : policy.priority,
              risk: changed.has(policy.risk.field) ? { ...policy.risk, values: {} } : policy.risk,
              allowed: Object.fromEntries(
                Object.entries(policy.allowed ?? {}).filter(([alias]) => !changed.has(alias)),
              ),
            }
          : policy,
      );
  }
  const snapshotSources = Object.entries(snapshotSelections);
  const snapshotFields = selectedIssueFields(snapshotSelections, catalog).map(
    ({ alias, field, selection }) => ({ alias, field, required: selection.required }),
  );
  const snapshotReady =
    !needs('issue_snapshot') ||
    (!!catalog &&
      !capabilities.isFetching &&
      !capabilities.error &&
      snapshotSources.length > 0 &&
      snapshotFields.length === snapshotSources.length &&
      snapshotFields.every(({ field, required }) => !required || field.required_supported) &&
      !!admission &&
      validIssueAdmission(admission, snapshotFields));
  const conditions = editor.choice_thresholds ?? {};
  const conditionsReady = Object.entries(conditions).every(([node, alias]) => {
    const field = snapshotFields.find((field) => field.alias === alias)?.field;
    return field?.shape.type === 'enum' && field.shape.values.includes(thresholds[node] ?? '');
  });
  const ready =
    snapshotReady &&
    formsReady &&
    (!needs('choice_threshold') || (Object.keys(conditions).length > 0 && conditionsReady)) &&
    (!needs('accepted_predecessor') || !!inputs.accepted_predecessor?.outcomes.length) &&
    (!needs('publication') || !!publisher) &&
    commandSteps.every(
      (step) => !commandRequired(step) || commandSelections.some(([alias]) => alias === step.alias),
    ) &&
    reasoningSteps.every((step) => !!profiles[step.alias]);

  const signature = JSON.stringify({
    projectId,
    preset: preset.reference,
    profiles,
    commands,
    target,
    admission,
    inputs,
    selectedForms,
    snapshotSelections,
    thresholds,
    catalogVersion: catalog?.definition_version,
    writer: capabilities.data?.issue_classification?.plugin,
  });
  const latestRef = useRef({ signature, ready });
  latestRef.current = { signature, ready };
  const [problem, setProblem] = useState(false);

  async function open() {
    if (!ready || busy) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setBusy(true);
    setError(undefined);
    setIssues([]);
    setProblem(false);
    const publication = publisher && {
      plugin: publisher.plugin,
      target: publisher.target,
      permissions: publisher.permissions,
    };
    const budget = { ...editor.budget };
    const permissions = [
      ...Object.values(profiles).map(({ profile, build }) => {
        for (const dimension of Object.keys(budget) as (keyof typeof budget)[]) {
          budget[dimension] = Math.max(budget[dimension], profile.definition.budget[dimension]);
        }
        return profile.definition.execution_binding === 'step'
          ? (build.execution_permissions ?? emptyFactoryPermissions())
          : profile.definition.permissions;
      }),
      ...commandSelections.map(([, selection]) => selection.permissions),
      ...(publication ? [publication.permissions] : []),
      {
        ...emptyFactoryPermissions(),
        plugins: Object.values(selectedForms).flatMap((form) => (form ? [form.writer] : [])),
      },
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
          ...(Object.keys(selectedForms).length
            ? {
                issue_fields: Object.fromEntries(
                  Object.entries(selectedForms).flatMap(([node, form]) =>
                    form ? [[node, form.form]] : [],
                  ),
                ),
              }
            : {}),
          ...(publication ? { publication } : {}),
          ...(needs('issue_snapshot') && catalog && admission
            ? {
                issue_snapshot: {
                  definition_version: catalog.definition_version,
                  admission,
                  fields: snapshotSelections,
                  ...(Object.keys(conditions).length ? { choice_thresholds: thresholds } : {}),
                },
              }
            : {}),
          ...(inputs.accepted_predecessor
            ? { accepted_predecessor: inputs.accepted_predecessor }
            : {}),
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      if (latestRef.current.signature !== signature || !latestRef.current.ready) {
        setProblem(true);
        return;
      }
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
      {needs('issue_snapshot') ? (
        <>
          {catalog ? (
            <p>{t('planning.definition', { version: catalog.definition_version })}</p>
          ) : null}
          {catalog ? (
            <SnapshotFieldSelectors
              fields={snapshotSelections}
              choices={catalog}
              onChange={changeSnapshotFields}
              conditions={conditions}
              thresholds={thresholds}
              onThresholdChange={setThresholds}
            />
          ) : null}
          {snapshotFields.length !== snapshotSources.length || !snapshotSources.length ? (
            <p role="alert">{t('factory.issueFieldsUnavailable')}</p>
          ) : null}
          <IssueAdmission value={admission} fields={snapshotFields} onChange={setAdmission} />
        </>
      ) : null}
      {Object.entries(selectedForms).map(([node, form]) => (
        <fieldset key={node}>
          <legend>{stepLabel(node)}</legend>
          {capabilities.isFetching ? (
            <p role="status">{t('factory.loadingStepCapabilities')}</p>
          ) : (
            <FillIssueFields
              projectId={projectId}
              choices={catalog}
              writer={capabilities.data?.issue_classification}
              value={form}
              onChange={(next) => setFieldForms((current) => ({ ...current, [node]: next }))}
            />
          )}
        </fieldset>
      ))}
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
      {problem ? <p role="alert">{t('factory.exampleDraftChanged')}</p> : null}
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
