import { factoryFieldId } from './fieldFocus';
import { FlowGraphOverview } from './FlowGraphOverview';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import { useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import type { MessageKey } from '../../app/presentation/messages';
import styles from './Factory.module.css';
import { ExecutionProfiles } from './ExecutionProfiles';
import { FlowSettings } from './FlowSettings';
import { FlowBindings } from './FlowBindings';
import type { FlowIssueServices } from './FlowIssueStatus';
import type { FlowSourceServices } from './FlowSourceUpdate';
import { FlowProjections } from './FlowProjections';
import { FlowTransitions } from './FlowTransitions';
import { BudgetEditor, PermissionEditor } from './ResourceEditors';

type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
interface Props {
  value: Draft;
  choices: readonly components['schemas']['FactoryConfigurationChoiceResource'][];
  subflows: readonly components['schemas']['FactoryFlowDefinitionDetails'][];
  onChange: (value: Draft) => void;
  sources?: FlowSourceServices;
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
  onChange,
  choices,
  subflows,
  sources,
  issueStatus,
}: Props) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState(() =>
    Math.max(
      0,
      value.root.nodes.findIndex((node) => node.key === value.root.entry),
    ),
  );
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
  const nodeKeys = useDraftRowKeys(value.root.nodes);
  const outcomeKeys = useDraftRowKeys(node?.outcomes ?? []);
  const updateNode = (patch: Partial<components['schemas']['FactoryFlowNodeDefinition']>) =>
    onChange({
      ...value,
      root: {
        ...value.root,
        nodes: value.root.nodes.map((item, index) =>
          index === selected ? { ...item, ...patch } : item,
        ),
      },
    });
  return (
    <section className={styles.editor} data-factory-field="/root" tabIndex={-1}>
      <h3>{t('factory.graphEditor')}</h3>
      <FlowGraphOverview value={value.root} />
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
      <div className={styles.nodeList}>
        {value.root.nodes.map((item, index) => (
          <button
            type="button"
            data-factory-node-index={index}
            aria-pressed={selected === index}
            key={nodeKeys[index]}
            onClick={() => setSelected(index)}
          >
            {item.key || t('factory.unnamedNode')}
          </button>
        ))}
      </div>
      <button
        type="button"
        onClick={() => {
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
                  required: true,
                  outcomes: [],
                  input_schema: null,
                  subflow: null,
                  budget: { ...value.root.execution.budget },
                  permissions: structuredClone(value.root.execution.permissions),
                },
              ],
            },
          });
        }}
      >
        {t('factory.addNode')}
      </button>
      {node === undefined ? null : (
        <fieldset data-factory-field={`/root/nodes/${selected}`} tabIndex={-1}>
          <legend>{t('factory.node')}</legend>
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
              target.kind = kind as keyof typeof NODE_KINDS;
              target.build = null;
              target.action = null;
              target.gate = null;
              target.subflow = null;
              target.operator_requests = null;
              onChange(next);
            }}
          />
          <ExecutionProfiles
            value={value}
            node={node}
            choices={choices}
            flows={subflows}
            onChange={(next) => {
              // Only profile selection resets binding drafts; applying their edits keeps the review.
              setBindingRevision((revision) => revision + 1);
              onChange(next);
            }}
          />
          <FlowBindings
            key={bindingRevision}
            issueStatus={issueStatus}
            sources={sources}
            node={node}
            root={value.root}
            schemas={value.data_schemas}
            onChange={(next) => updateNode(next)}
          />
          <BudgetEditor
            label={t('factory.nodeBudget')}
            value={node.budget}
            ceiling={value.root.execution.budget}
            onChange={(budget) => updateNode({ budget })}
          />
          <PermissionEditor
            label={t('factory.nodePermissions')}
            value={node.permissions}
            ceiling={value.root.execution.permissions}
            choices={choices}
            onChange={(permissions) => updateNode({ permissions })}
          />
          <h4>{t('factory.outcomes')}</h4>
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
  if (next.root.entry === old) next.root.entry = name;
  for (const edge of next.root.transitions) {
    if (edge.predecessor === old) edge.predecessor = name;
    if (edge.target.kind === 'node' && edge.target.key === old) edge.target.key = name;
  }
  for (const edge of [...next.root.context_projections, ...next.root.data_projections]) {
    if (edge.predecessor === old) edge.predecessor = name;
    if (edge.successor === old) edge.successor = name;
  }
  for (const target of next.root.nodes)
    for (const source of Object.values(target.input_binding?.sources ?? {}))
      if (source.source === 'record' && source.node === old) source.node = name;
  return next;
}
