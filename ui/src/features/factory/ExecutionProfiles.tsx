import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Choice = components['schemas']['FactoryConfigurationChoiceResource'];
type Flow = components['schemas']['FactoryFlowDefinitionDetails'];

/** Reuses exact published execution bindings, never reconstructing provider payloads or digests. */
export function ExecutionProfiles({
  value,
  node,
  choices,
  flows,
  onChange,
}: {
  value: Draft;
  node: Node;
  choices: readonly Choice[];
  flows: readonly Flow[];
  onChange: (draft: Draft) => void;
}) {
  const { t } = usePresentation();
  const profiles = flows
    .flatMap((flow) =>
      flow.flow.closure.definitions.flatMap((definition) =>
        definition.nodes
          .filter((item) => item.kind === node.kind && (item.build || item.action || item.gate))
          .map((item) => ({ flow, definition, node: item })),
      ),
    )
    .filter((profile) => available(profile.node, choices, profile.flow.native_gate_policies ?? []));
  const update = (patch: Partial<Node>) =>
    onChange({
      ...value,
      root: {
        ...value.root,
        nodes: value.root.nodes.map((item) => (item === node ? { ...item, ...patch } : item)),
      },
    });
  return (
    <section>
      {node.kind === 'subflow_call' ? (
        <SelectMenu
          ariaLabel={t('factory.subflow')}
          value={String(
            flows.findIndex(
              (flow) =>
                flow.flow.closure.root.id === node.subflow?.id &&
                flow.flow.closure.root.version === node.subflow.version,
            ),
          )}
          options={[
            { value: '-1', label: t('factory.none') },
            ...flows.map((flow, index) => ({
              value: String(index),
              label: `${flow.flow.closure.definitions.find((definition) => definition.reference.id === flow.flow.closure.root.id)?.entry ?? t('factory.immutableFlow')} · v${flow.flow.closure.root.version}`,
            })),
          ]}
          onValueChange={(index) => {
            const flow = flows[Number(index)];
            onChange({
              ...value,
              data_schemas: mergeSchemas(value.data_schemas, flow?.flow.data_schemas ?? []),
              root: {
                ...value.root,
                nodes: value.root.nodes.map((item) =>
                  item === node ? { ...item, subflow: flow?.flow.closure.root ?? null } : item,
                ),
              },
            });
          }}
        />
      ) : null}
      {(
        [
          'build_command',
          'reasoning',
          'decision_signal',
          'deterministic_gate',
          'trusted_action',
        ] as const
      ).some((kind) => kind === node.kind) ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.executionProfile')}
            value={String(profiles.findIndex((profile) => sameBinding(node, profile.node)))}
            options={[
              { value: '-1', label: t('factory.none') },
              ...profiles.map((profile, index) => ({
                value: String(index),
                label: `${profile.definition.entry} · ${profile.node.key} · v${profile.definition.reference.version}`,
              })),
            ]}
            onValueChange={(index) => {
              const profile = profiles[Number(index)];
              if (!profile) {
                update({ build: null, action: null, gate: null });
                return;
              }
              onChange({
                ...value,
                data_schemas: mergeSchemas(value.data_schemas, profile.flow.flow.data_schemas),
                root: {
                  ...value.root,
                  nodes: value.root.nodes.map((item) =>
                    item === node
                      ? {
                          ...item,
                          build: structuredClone(profile.node.build ?? null),
                          action: structuredClone(profile.node.action ?? null),
                          gate: structuredClone(profile.node.gate ?? null),
                          input_schema: profile.node.input_schema,
                          outcomes: structuredClone(profile.node.outcomes),
                        }
                      : item,
                  ),
                },
              });
            }}
          />
          {profiles.length === 0 ? <p>{t('factory.noProfiles')}</p> : null}
          {node.build ? (
            <p>
              {node.build.model_or_tool.identity} · {node.build.model_or_tool.version}
            </p>
          ) : null}
          {node.action ? (
            <p>
              {node.action.plugin.identity} · {node.action.plugin.version}
            </p>
          ) : null}
          {node.gate ? (
            <p>
              {node.gate.policy.identity} · {node.gate.policy.version}
            </p>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
function mergeSchemas(current: Draft['data_schemas'], incoming: Draft['data_schemas']) {
  return [
    ...new Map(
      [...current, ...incoming].map((schema) => [
        `${schema.reference.identity}:${schema.reference.version}:${schema.reference.digest}`,
        schema,
      ]),
    ).values(),
  ];
}
function sameBinding(left: Node, right: Node) {
  return (
    JSON.stringify([left.build, left.action, left.gate]) ===
    JSON.stringify([right.build, right.action, right.gate])
  );
}
function available(
  node: Node,
  choices: readonly Choice[],
  nativeGatePolicies: readonly components['schemas']['FactoryImmutableReferenceResource'][],
) {
  const nativeGate =
    node.gate &&
    nativeGatePolicies.some(
      (reference) =>
        reference.identity === node.gate?.policy.identity &&
        reference.version === node.gate.policy.version &&
        reference.digest === node.gate.policy.digest,
    );
  const immutable = [
    node.build?.plugin,
    node.build?.tool,
    node.build?.model_or_tool,
    node.build?.result_tool,
    ...(node.build?.evidence ?? []).flatMap((evidence) => [evidence.tool, evidence.plugin]),
    node.action?.plugin,
    ...(nativeGate ? [] : [node.gate?.policy]),
  ].filter((item) => item != null);
  return (
    immutable.every((reference) =>
      choices.some(
        (choice) =>
          choice.reference.kind === 'immutable' &&
          choice.reference.reference.identity === reference.identity &&
          choice.reference.reference.version === reference.version &&
          choice.reference.reference.digest === reference.digest,
      ),
    ) &&
    (!node.build ||
      choices.some(
        (choice) =>
          choice.reference.kind === 'build_configuration' &&
          choice.reference.reference.id === node.build?.build_configuration.id &&
          choice.reference.reference.project_id === node.build.build_configuration.project_id &&
          choice.reference.reference.version === node.build.build_configuration.version &&
          choice.reference.reference.definition_digest ===
            node.build.build_configuration.definition_digest,
      ))
  );
}
