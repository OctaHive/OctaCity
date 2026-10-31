import { FlowCodeSources, type CodeResourceServices } from './FlowCodeSources';
import { StepTask } from './StepTask';
import { StepCode } from './StepCode';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { factoryFieldId } from './fieldFocus';
import { FlowCanvas } from './FlowCanvas';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import { useEffect, useRef, useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import type { MessageKey } from '../../app/presentation/messages';
import styles from './Factory.module.css';
import type { RoleServices } from './StageProfiles';
import { FlowPrimitives, type FlowAuthoringServices, type PrimitiveDraft } from './FlowPrimitives';
import { StepExecutor } from './StepExecutor';
import { ExecutionProfiles } from './ExecutionProfiles';
import { FlowSettings } from './FlowSettings';
import { FlowBindings } from './FlowBindings';
import type { FlowIssueServices } from './FlowIssueStatus';
import type { FlowSourceServices } from './FlowSourceUpdate';
import { FlowProjections } from './FlowProjections';
import { FlowTransitions } from './FlowTransitions';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';
import { FlowDocuments, type FlowDocumentServices } from './FlowDocuments';
import { FlowProcess } from './FlowProcess';

type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
interface Props {
  onStepEditingChange?: ((editing: boolean) => void) | undefined;
  codeResources?: CodeResourceServices | undefined;
  authoring?: FlowAuthoringServices | undefined;
  value: Draft;
  choices: readonly components['schemas']['FactoryConfigurationChoiceResource'][];
  subflows: readonly components['schemas']['FactoryFlowDefinitionDetails'][];
  onChange: (value: Draft) => void;
  sources?: FlowSourceServices;
  roles?: RoleServices | undefined;
  documents?: FlowDocumentServices | undefined;
  onProcessEditingChange?: ((editing: boolean) => void) | undefined;
  issueStatus?: FlowIssueServices | undefined;
}

const NODE_KINDS = {
  build_command: 'factory.kind.build_command',
  reasoning: 'factory.kind.reasoning',
  decision_signal: 'factory.kind.decision_signal',
  deterministic_gate: 'factory.kind.deterministic_gate',
  fan_out: 'factory.kind.fan_out',
  join: 'factory.kind.join',
  human_gate: 'factory.kind.human_gate',
  trusted_action: 'factory.kind.trusted_action',
  subflow_call: 'factory.kind.subflow_call',
} satisfies Record<components['schemas']['FactoryFlowNodeKind'], MessageKey>;

/** A component-memory draft over the closed technical grammar; names have no special meaning. */
export function FlowGraphEditor({
  value,
  codeResources,
  authoring,
  onChange,
  choices,
  subflows,
  sources,
  issueStatus,
  roles,
  documents,
  onProcessEditingChange,
  onStepEditingChange,
}: Props) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState(() =>
    Math.max(
      0,
      value.root.nodes.findIndex((node) => node.key === value.root.entry),
    ),
  );
  const nodeKeys = useDraftRowKeys(value.root.nodes);
  const [stepDrafts, setStepDrafts] = useState<Record<string, PrimitiveDraft>>({});
  const isEditing = nodeKeys.some((key) => !!stepDrafts[key]);
  useEffect(() => {
    onStepEditingChange?.(isEditing);
  }, [isEditing, onStepEditingChange]);
  const changeStepDraft = (draft: PrimitiveDraft | undefined) =>
    setStepDrafts((current) => {
      const next = { ...current };
      const key = nodeKeys[selected];
      if (key) {
        if (draft) next[key] = draft;
        else delete next[key];
      }
      return next;
    });
  const [settings, setSettings] = useState(false);
  const [bindingRevision, setBindingRevision] = useState(0);
  const [admissionDeclarations] = useState(() => structuredClone(value.limits.permissions));
  const [permissionPreset, setPermissionPreset] = useState('');
  const permissionPresets = choices.filter(
    (choice) => choice.kind === 'permission_ceiling' && choice.metadata?.permission_declarations,
  );
  const permissionDeclarations =
    permissionPresets.find((choice) => choice.identity === permissionPreset)?.metadata
      ?.permission_declarations ?? admissionDeclarations;
  const node = value.root.nodes[selected];
  const outcomeKeys = useDraftRowKeys(node?.outcomes ?? []);
  const latestValueRef = useRef(value);
  latestValueRef.current = value;
  const updateNode = (
    patch: Partial<components['schemas']['FactoryFlowNodeDefinition']>,
    schemas: components['schemas']['FactoryFlowDataSchema'][] = [],
  ) =>
    onChange({
      ...latestValueRef.current,
      data_schemas: [
        ...latestValueRef.current.data_schemas,
        ...schemas.filter(
          (schema) =>
            !latestValueRef.current.data_schemas.some(
              (existing) => existing.reference.digest === schema.reference.digest,
            ),
        ),
      ],
      root: {
        ...latestValueRef.current.root,
        nodes: latestValueRef.current.root.nodes.map((item, index) =>
          index === selected ? { ...item, ...patch } : item,
        ),
      },
    });
  const addStep = () => {
    setSelected(value.root.nodes.length);
    onChange({
      ...value,
      root: {
        ...value.root,
        nodes: [
          ...value.root.nodes,
          {
            key: '',
            kind: 'human_gate',
            code: { kind: 'none' },
            required: true,
            outcomes: [],
            input_schema: null,
            subflow: null,
            budget: { ...value.root.execution.budget },
            permissions: emptyFactoryPermissions(),
          },
        ],
      },
    });
  };
  const copiedExecution = node ? (
    <ExecutionProfiles
      value={value}
      node={node}
      choices={choices}
      flows={subflows}
      onChange={(next) => {
        setBindingRevision((revision) => revision + 1);
        onChange(next);
      }}
    />
  ) : null;
  return (
    <section className={styles.editor} data-factory-field="/root" tabIndex={-1}>
      <h3>{t('factory.graphEditor')}</h3>
      {documents ? (
        <FlowProcess
          value={value}
          services={documents}
          onChange={onChange}
          onEditingChange={onProcessEditingChange}
        />
      ) : null}
      {codeResources ? (
        <FlowCodeSources
          root={value.root}
          services={codeResources}
          onChange={(root) => onChange({ ...value, root })}
        />
      ) : null}
      <FlowCanvas
        onAddStep={addStep}
        value={value.root}
        selected={selected}
        onSelect={setSelected}
        repeatCeiling={value.limits.max_repeat_count}
        onChange={(root) => onChange({ ...value, root })}
      />
      <button
        type="button"
        data-factory-settings
        aria-expanded={settings}
        onClick={() => setSettings(!settings)}
      >
        {t('factory.flowSettings')}
      </button>
      {settings ? (
        <>
          {permissionPresets.length ? (
            <SelectMenu
              ariaLabel={t('factory.permissionPreset')}
              value={permissionPreset}
              options={[
                { value: '', label: t('factory.baselineDeclarations') },
                ...permissionPresets.map((choice) => ({
                  value: choice.identity,
                  label: `${choice.alias} · ${choice.reference.reference.version}`,
                })),
              ]}
              onValueChange={setPermissionPreset}
            />
          ) : null}
          <FlowSettings
            value={value}
            choices={choices}
            permissionDeclarations={permissionDeclarations}
            onChange={onChange}
          />
        </>
      ) : null}
      {node === undefined ? null : (
        <fieldset data-factory-field={`/root/nodes/${selected}`} tabIndex={-1}>
          <legend>{t('factory.node')}</legend>
          {stepDrafts[nodeKeys[selected] ?? ''] ? (
            <>
              <p role="status">{t('factory.unappliedStepChanges')}</p>
              <button
                type="button"
                onClick={() => {
                  changeStepDraft(undefined);
                  setBindingRevision((revision) => revision + 1);
                }}
              >
                {t('factory.discardStepChanges')}
              </button>
            </>
          ) : null}
          <label>
            <input
              type="checkbox"
              checked={node.required}
              onChange={(event) => updateNode({ required: event.target.checked })}
            />
            {t('factory.requiredBranch')}
          </label>
          <label>
            {t('factory.executionRetries')}
            <input
              type="number"
              min={0}
              max={value.root.execution.budget.max_attempts}
              value={node.max_execution_retries ?? 0}
              onChange={(event) =>
                updateNode({ max_execution_retries: Number(event.target.value) })
              }
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.namedPool')}
            value={node.phase_pool ?? ''}
            options={[
              { value: '', label: t('factory.none') },
              ...Object.keys(value.pools ?? {}).map((name) => ({ value: name, label: name })),
            ]}
            onValueChange={(phase_pool) => updateNode({ phase_pool: phase_pool || null })}
          />

          <button
            type="button"
            onClick={() => {
              const key = node.key;
              onChange({
                ...value,
                root: {
                  ...value.root,
                  ...(value.root.process
                    ? { process: updateProcessRecipients(value.root.process, key, null) }
                    : {}),
                  entry: value.root.entry === key ? '' : value.root.entry,
                  nodes: value.root.nodes.filter((_, index) => index !== selected),
                  transitions: value.root.transitions.filter(
                    (edge) =>
                      edge.predecessor !== key &&
                      !(edge.target.kind === 'node' && edge.target.key === key),
                  ),
                  context_projections: value.root.context_projections.filter(
                    (edge) => edge.predecessor !== key && edge.successor !== key,
                  ),
                  data_projections: value.root.data_projections.filter(
                    (edge) => edge.predecessor !== key && edge.successor !== key,
                  ),
                },
              });
              setSelected(Math.max(0, selected - 1));
            }}
          >
            {t('factory.removeNode')}
          </button>
          <label>
            {t('factory.nodeName')}
            <input
              id={factoryFieldId(`/root/nodes/${selected}/key`)}
              value={node.key}
              onChange={(event) => onChange(renameNode(value, selected, event.target.value))}
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.nodeKind')}
            value={node.kind}
            options={Object.entries(NODE_KINDS).map(([kind, label]) => ({
              value: kind,
              label: t(label),
            }))}
            onValueChange={(kind) => {
              const next = structuredClone(value);
              const target = next.root.nodes[selected];
              if (target === undefined || !Object.hasOwn(NODE_KINDS, kind)) return;
              changeStepDraft(undefined);
              target.kind = kind as keyof typeof NODE_KINDS;
              if (target.kind !== 'reasoning') target.task = null;
              target.profile_selection = null;
              target.build = null;
              target.action = null;
              target.gate = null;
              target.subflow = null;
              target.operator_requests = null;
              onChange(next);
            }}
          />
          {node.kind === 'subflow_call' ? (
            copiedExecution
          ) : subflows.length ? (
            <details>
              <summary>{t('factory.reuseStepContracts')}</summary>
              {copiedExecution}
            </details>
          ) : null}
          <fieldset>
            <legend>{t('factory.expectedResult')}</legend>
            <p>{t('factory.expectedResultHelp')}</p>
            <button
              type="button"
              disabled={value.data_schemas.length === 0}
              onClick={() => {
                const schema = value.data_schemas[0];
                if (schema)
                  updateNode({
                    outcomes: [
                      ...node.outcomes,
                      { key: '', kind: 'success', schema: schema.reference },
                    ],
                  });
              }}
            >
              {t('factory.addOutcome')}
            </button>
            {node.outcomes.map((outcome, index) => (
              <fieldset key={outcomeKeys[index]}>
                <label>
                  {t('factory.outcomeName')}
                  <input
                    value={outcome.key}
                    onChange={(event) =>
                      updateNode({
                        outcomes: node.outcomes.map((item, position) =>
                          position === index ? { ...item, key: event.target.value } : item,
                        ),
                      })
                    }
                  />
                </label>
                <SelectMenu
                  ariaLabel={t('factory.outcomeKind')}
                  value={outcome.kind}
                  options={[
                    { value: 'success', label: t('factory.success') },
                    { value: 'failure', label: t('factory.failure') },
                  ]}
                  onValueChange={(kind) => {
                    if (kind === 'success' || kind === 'failure')
                      updateNode({
                        outcomes: node.outcomes.map((item, position) =>
                          position === index ? { ...item, kind } : item,
                        ),
                      });
                  }}
                />
                <SelectMenu
                  ariaLabel={t('factory.outcomeSchema')}
                  value={String(
                    value.data_schemas.findIndex(
                      (schema) => schema.reference.digest === outcome.schema.digest,
                    ),
                  )}
                  options={value.data_schemas.map((schema, position) => ({
                    value: String(position),
                    label: `${schema.reference.identity} · ${schema.reference.version}`,
                  }))}
                  onValueChange={(selected) => {
                    const schema = value.data_schemas[Number(selected)];
                    if (schema)
                      updateNode({
                        outcomes: node.outcomes.map((item, position) =>
                          position === index ? { ...item, schema: schema.reference } : item,
                        ),
                      });
                  }}
                />
              </fieldset>
            ))}
          </fieldset>
          {authoring &&
          (node.kind === 'deterministic_gate' ||
            node.kind === 'human_gate' ||
            node.kind === 'build_command') ? (
            <FlowPrimitives
              key={`${selected}:${node.kind}:${bindingRevision}`}
              draft={stepDrafts[nodeKeys[selected] ?? '']}
              onDraftChange={changeStepDraft}
              node={node}
              services={authoring}
              onChange={updateNode}
            />
          ) : null}
          <StepTask node={node} onChange={updateNode} />
          <StepExecutor
            key={`${nodeKeys[selected]}:${bindingRevision}`}
            node={node}
            choices={choices}
            services={roles}
            onChange={updateNode}
          />
          <FlowBindings
            sourceDraft={stepDrafts[nodeKeys[selected] ?? '']?.reporter}
            onSourceDraftChange={(reporter) => changeStepDraft(reporter ? { reporter } : undefined)}
            key={bindingRevision}
            documentsManaged={!!documents}
            issueStatus={issueStatus}
            sources={sources}
            node={node}
            root={value.root}
            schemas={value.data_schemas}
            onChange={(next, schemas) => updateNode(next, schemas)}
          />
          {codeResources ? (
            <StepCode
              authoring={authoring}
              node={node}
              root={value.root}
              services={codeResources}
              onChange={updateNode}
            />
          ) : null}
          {documents ? (
            <FlowDocuments
              key={node.key}
              node={node}
              services={documents}
              onChange={updateNode}
              processManaged={value.root.process != null}
            />
          ) : null}
          <BudgetEditor
            fieldPath={`/root/nodes/${selected}/budget`}
            label={t('factory.nodeBudget')}
            value={node.budget}
            ceiling={value.root.execution.budget}
            onChange={(budget) => updateNode({ budget })}
          />
          <PermissionEditor
            fieldPath={`/root/nodes/${selected}/permissions`}
            label={t('factory.nodePermissions')}
            value={node.permissions}
            ceiling={value.root.execution.permissions}
            choices={choices}
            onChange={(permissions) => updateNode({ permissions })}
          />
        </fieldset>
      )}
      <FlowProjections value={value.root} onChange={(root) => onChange({ ...value, root })} />
      <FlowTransitions
        repeatCeiling={value.limits.max_repeat_count}
        value={value.root}
        onChange={(root) => onChange({ ...value, root })}
      />
    </section>
  );
}

function renameNode(value: Draft, index: number, name: string): Draft {
  const next = structuredClone(value);
  const node = next.root.nodes[index];
  if (node === undefined) return next;
  const old = node.key;
  node.key = name;
  if (next.root.process) next.root.process = updateProcessRecipients(next.root.process, old, name);
  if (next.root.entry === old) next.root.entry = name;
  for (const edge of next.root.transitions) {
    if (edge.predecessor === old) edge.predecessor = name;
    if (edge.target.kind === 'node' && edge.target.key === old) edge.target.key = name;
  }
  for (const edge of [...next.root.context_projections, ...next.root.data_projections]) {
    if (edge.predecessor === old) edge.predecessor = name;
    if (edge.successor === old) edge.successor = name;
  }
  for (const target of next.root.nodes) {
    if (
      target.code?.kind === 'source' &&
      target.code.revision.kind === 'accepted_candidate' &&
      target.code.revision.node === old
    )
      target.code = { ...target.code, revision: { ...target.code.revision, node: name } };
    for (const source of Object.values(target.input_binding?.sources ?? {}))
      if (source.source === 'record' && source.node === old) source.node = name;
  }
  return next;
}

function updateProcessRecipients(
  process: NonNullable<Draft['root']['process']>,
  old: string,
  replacement: string | null,
) {
  const next = { ...process };
  for (const kind of ['goals', 'criteria'] as const) {
    const document = process[kind];
    if (document)
      next[kind] = {
        ...document,
        recipients:
          replacement === null
            ? document.recipients.filter((key) => key !== old)
            : document.recipients.map((key) => (key === old ? replacement : key)),
      };
  }
  return next;
}
