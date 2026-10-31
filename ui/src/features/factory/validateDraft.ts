import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { validFactoryValue } from '../../shared/FactoryValueInput';
import { exactSchemaInteger } from '../../shared/factoryIntegerBound';

export type DraftIssue = {
  field: string;
  code:
    | 'invalid_contract'
    | 'unavailable_reference'
    | 'unbounded_cycle'
    | 'unreachable_node'
    | 'incompatible_schema'
    | 'budget_exceeded';
};
type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
type Contract = {
  readonly description?: string;
  readonly $ref?: string;
  readonly type?: string | readonly string[];
  readonly const?: unknown;
  readonly enum?: readonly unknown[];
  readonly oneOf?: readonly Contract[];
  readonly anyOf?: readonly Contract[];
  readonly allOf?: readonly Contract[];
  readonly properties?: Readonly<Record<string, Contract>>;
  readonly required?: readonly string[];
  readonly items?: Contract;
  readonly additionalProperties?: boolean | Contract;
  readonly propertyNames?: Contract;
  readonly maxProperties?: number;
  readonly minProperties?: number;
  readonly minLength?: number;
  readonly maxLength?: number;
  readonly pattern?: string;
  readonly minimum?: number;
  readonly maximum?: number;
  readonly minItems?: number;
  readonly maxItems?: number;
  readonly uniqueItems?: boolean;
  readonly 'x-max-utf8-bytes'?: number;
};
const contracts: Readonly<Record<string, Contract>> = FACTORY_AUTHORING_SCHEMAS;
const pointer = (value: string) => value.replaceAll('~', '~0').replaceAll('/', '~1');
/** Reads retained declarative parameters using the generated contract, without claiming server approval. */
export function isFactoryGateProgram(
  value: unknown,
): value is components['schemas']['FactoryDeclarativeGateParameters'] {
  return validateContract(value, contracts.FactoryDeclarativeGateParameters!, '').length === 0;
}

/** Early usability checks only. The authoritative server validates again before publication. */
export function validateFlowDraft(value: Draft): DraftIssue[] {
  const issues = validateContract(value, contracts.FactoryFlowDefinitionDraft!, '');
  const add = (field: string, code: DraftIssue['code']) => issues.push({ field, code });
  const nodes = new Map(value.root.nodes.map((node) => [node.key, node]));
  const terminals = new Set(value.root.terminals.map((terminal) => terminal.key));
  const schemas = new Map(
    value.data_schemas.map((schema) => [referenceKey(schema.reference), schema.shape]),
  );
  const schema = (
    reference: components['schemas']['FactoryImmutableReferenceResource'] | null,
    field: string,
  ) => {
    if (reference && !schemas.has(referenceKey(reference))) add(field, 'unavailable_reference');
  };
  if (!nodes.has(value.root.entry)) add('/root/entry', 'unavailable_reference');
  schema(value.root.input_schema, '/root/input_schema');
  const names = new Set<string>();
  value.root.nodes.forEach((node, index) => {
    const field = `/root/nodes/${index}`;
    if (names.has(node.key)) add(`${field}/key`, 'invalid_contract');
    names.add(node.key);
    schema(node.input_schema, `${field}/input_schema`);
    const outcomes = new Set<string>();
    node.outcomes.forEach((outcome, position) => {
      if (outcomes.has(outcome.key)) add(`${field}/outcomes/${position}/key`, 'invalid_contract');
      outcomes.add(outcome.key);
      schema(outcome.schema, `${field}/outcomes/${position}/schema`);
    });
    for (const key of Object.keys(node.budget) as (keyof typeof node.budget)[])
      if (node.budget[key] > value.root.execution.budget[key])
        add(`${field}/budget/${key}`, 'budget_exceeded');
    if (node.kind === 'subflow_call' && !node.subflow)
      add(`${field}/subflow`, 'unavailable_reference');
    if (
      (node.kind === 'build_command' || node.kind === 'reasoning') &&
      !node.build &&
      !(node.kind === 'reasoning' && node.profile_selection)
    )
      add(`${field}/build`, 'unavailable_reference');
    if (node.kind === 'trusted_action' && !node.action && !node.build)
      add(`${field}/action`, 'unavailable_reference');
    if (
      (node.kind === 'deterministic_gate' || node.kind === 'decision_signal') &&
      !node.gate &&
      !node.build
    )
      add(`${field}/gate`, 'unavailable_reference');
  });
  const terminalNames = new Set<string>();
  value.root.terminals.forEach((terminal, index) => {
    if (terminalNames.has(terminal.key)) add(`/root/terminals/${index}/key`, 'invalid_contract');
    terminalNames.add(terminal.key);
    schema(terminal.schema, `/root/terminals/${index}/schema`);
  });
  value.root.transitions.forEach((edge, index) => {
    const field = `/root/transitions/${index}`;
    if (!nodes.has(edge.predecessor)) add(`${field}/predecessor`, 'unavailable_reference');
    if (!nodes.get(edge.predecessor)?.outcomes.some((outcome) => outcome.key === edge.outcome))
      add(`${field}/outcome`, 'unavailable_reference');
    if (edge.target.kind === 'node' ? !nodes.has(edge.target.key) : !terminals.has(edge.target.key))
      add(`${field}/target`, 'unavailable_reference');
    if (
      edge.max_repeats > value.limits.max_repeat_count ||
      (edge.max_repeats > 0 &&
        (!edge.exhausted_terminal || !terminals.has(edge.exhausted_terminal)))
    )
      add(`${field}/max_repeats`, 'invalid_contract');
  });
  const reachable = new Set<string>();
  const pending = [value.root.entry];
  while (pending.length) {
    const key = pending.pop()!;
    if (reachable.has(key)) continue;
    reachable.add(key);
    for (const edge of value.root.transitions)
      if (edge.predecessor === key && edge.target.kind === 'node') pending.push(edge.target.key);
  }
  value.root.nodes.forEach((node, index) => {
    if (!reachable.has(node.key)) add(`/root/nodes/${index}`, 'unreachable_node');
  });
  // Removing bounded edges must leave an acyclic graph: every cycle needs an explicit finite exit.
  const active = new Set<string>(),
    done = new Set<string>();
  function visit(key: string) {
    if (done.has(key)) return;
    active.add(key);
    for (const [index, edge] of value.root.transitions.entries()) {
      if (edge.predecessor !== key || edge.target.kind !== 'node' || edge.max_repeats > 0) continue;
      if (active.has(edge.target.key)) add(`/root/transitions/${index}`, 'unbounded_cycle');
      else visit(edge.target.key);
    }
    active.delete(key);
    done.add(key);
  }
  for (const key of nodes.keys()) visit(key);
  for (const [kind, edges] of [
    ['context_projections', value.root.context_projections],
    ['data_projections', value.root.data_projections],
  ] as const)
    edges.forEach((edge, index) => {
      const field = `/root/${kind}/${index}`;
      const predecessor = nodes.get(edge.predecessor);
      const successor = nodes.get(edge.successor);
      const outcome = predecessor?.outcomes.find((outcome) => outcome.key === edge.outcome);
      if (!predecessor || !successor || !outcome) add(field, 'unavailable_reference');
      if (
        'selections' in edge &&
        edge.selections.some(
          (name) => !Object.hasOwn(predecessor?.context_binding?.entries ?? {}, name),
        )
      )
        add(`${field}/selections`, 'unavailable_reference');
      if ('schema' in edge) {
        schema(edge.schema, `${field}/schema`);
        if (outcome && referenceKey(outcome.schema) !== referenceKey(edge.schema))
          add(`${field}/schema`, 'incompatible_schema');
        if (successor?.input_schema && !successor.input_binding) {
          const from = schemas.get(referenceKey(edge.schema)),
            to = schemas.get(referenceKey(successor.input_schema));
          if (from && to && !compatibleFactoryShape(from, to))
            add(`${field}/schema`, 'incompatible_schema');
        }
      }
    });
  for (const key of Object.keys(
    value.root.execution.budget,
  ) as (keyof typeof value.limits.budget)[])
    if (value.root.execution.budget[key] > value.limits.budget[key])
      add(`/root/execution/budget/${key}`, 'budget_exceeded');
  return unique(issues);
}
export function validateConfigurationDraft(
  value: components['schemas']['FactoryConfigurationDefinition'],
): DraftIssue[] {
  return unique(validateContract(value, contracts.FactoryConfigurationDefinition!, ''));
}
function referenceKey(reference: components['schemas']['FactoryImmutableReferenceResource']) {
  return `${reference.identity}:${reference.version}:${reference.digest}`;
}
function unique(issues: DraftIssue[]) {
  return issues
    .filter(
      (issue, index) =>
        issues.findIndex((item) => item.field === issue.field && item.code === issue.code) ===
        index,
    )
    .slice(0, 64);
}
function validateContract(
  value: unknown,
  contract: Contract,
  field: string,
  depth = 0,
): DraftIssue[] {
  const invalid = () => [{ field, code: 'invalid_contract' as const }];
  if (depth > 128) return invalid();
  if (contract.$ref) {
    const next = contracts[contract.$ref.split('/').at(-1)!];
    return next ? validateContract(value, next, field, depth + 1) : invalid();
  }
  const variants = contract.oneOf ?? contract.anyOf;
  if (variants) {
    const results = variants.map((schema) => validateContract(value, schema, field, depth + 1));
    const passing = results.filter((result) => result.length === 0);
    if (passing.length && (!contract.oneOf || passing.length === 1)) return [];
    return results.sort((a, b) => a.length - b.length)[0] ?? invalid();
  }
  if (contract.allOf)
    return contract.allOf.flatMap((schema) => validateContract(value, schema, field, depth + 1));
  if (Object.hasOwn(contract, 'const') && JSON.stringify(value) !== JSON.stringify(contract.const))
    return invalid();
  if (
    contract.enum &&
    !contract.enum.some((item) => JSON.stringify(item) === JSON.stringify(value))
  )
    return invalid();
  const types = typeof contract.type === 'string' ? [contract.type] : contract.type;
  if (
    types &&
    !types.some((type) =>
      type === 'null'
        ? value === null
        : type === 'array'
          ? Array.isArray(value)
          : type === 'object'
            ? value !== null && typeof value === 'object' && !Array.isArray(value)
            : type === 'integer'
              ? typeof value === 'number' && Number.isSafeInteger(value)
              : type === 'number'
                ? typeof value === 'number' && Number.isFinite(value)
                : typeof value === type,
    )
  )
    return invalid();
  if (
    typeof value === 'number' &&
    (!Number.isFinite(value) ||
      (contract.minimum !== undefined && value < contract.minimum) ||
      (contract.maximum !== undefined && value > contract.maximum))
  )
    return invalid();
  if (
    typeof value === 'string' &&
    ((contract.minLength !== undefined && [...value].length < contract.minLength) ||
      (contract.maxLength !== undefined && [...value].length > contract.maxLength) ||
      (contract.pattern && !new RegExp(contract.pattern, 'u').test(value)) ||
      (contract['x-max-utf8-bytes'] !== undefined &&
        new TextEncoder().encode(value).length > contract['x-max-utf8-bytes']))
  )
    return invalid();
  if (Array.isArray(value)) {
    if (
      (contract.minItems !== undefined && value.length < contract.minItems) ||
      (contract.maxItems !== undefined && value.length > contract.maxItems)
    )
      return invalid();
    return contract.items
      ? value.flatMap((item, index) =>
          validateContract(item, contract.items!, `${field}/${index}`, depth + 1),
        )
      : [];
  }
  if (value !== null && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    const entries = Object.entries(record);
    const issues: DraftIssue[] = [];
    if (
      (contract.minProperties !== undefined && entries.length < contract.minProperties) ||
      (contract.maxProperties !== undefined && entries.length > contract.maxProperties)
    )
      return invalid();
    for (const key of contract.required ?? [])
      if (!Object.hasOwn(record, key))
        issues.push({ field: `${field}/${pointer(key)}`, code: 'invalid_contract' });
    for (const [key, item] of entries) {
      const next = contract.properties?.[key];
      const path = `${field}/${pointer(key)}`;
      if (contract.propertyNames)
        issues.push(...validateContract(key, contract.propertyNames, path, depth + 1));
      if (next) issues.push(...validateContract(item, next, path, depth + 1));
      else if (contract.additionalProperties === false)
        issues.push({ field: path, code: 'invalid_contract' });
      else if (typeof contract.additionalProperties === 'object')
        issues.push(...validateContract(item, contract.additionalProperties, path, depth + 1));
    }
    return issues;
  }
  return [];
}
/** Structural usability hint; publication and execution still validate the exact schemas. */
export function compatibleFactoryShape(
  from: components['schemas']['FactoryFlowValueSchema'],
  to: components['schemas']['FactoryFlowValueSchema'],
): boolean {
  if (from.type === 'enum' && to.type === 'externally_tagged')
    return from.values.every((value) => validFactoryValue(to, value));
  if (from.type !== to.type) return false;
  if (from.type === 'boolean' && to.type === 'boolean') return true;
  if (from.type === 'integer' && to.type === 'integer') {
    const bounds = [from.minimum, from.maximum, to.minimum, to.maximum].map(exactSchemaInteger);
    const [low, high, targetLow, targetHigh] = bounds;
    return (
      low != null &&
      high != null &&
      targetLow != null &&
      targetHigh != null &&
      low >= targetLow &&
      high <= targetHigh
    );
  }
  if (from.type === 'string' && to.type === 'string')
    return from.min_bytes >= to.min_bytes && from.max_bytes <= to.max_bytes;
  if (from.type === 'enum' && to.type === 'enum')
    return from.values.every((value) => to.values.includes(value));
  if (from.type === 'array' && to.type === 'array')
    return (
      from.min_items >= to.min_items &&
      from.max_items <= to.max_items &&
      compatibleFactoryShape(from.item, to.item)
    );
  if (from.type === 'externally_tagged' && to.type === 'externally_tagged')
    return Object.entries(from.variants).every(([name, shape]) => {
      if (!Object.hasOwn(to.variants, name)) return false;
      const target = to.variants[name];
      return shape === null
        ? target === null
        : target !== null && target !== undefined && compatibleFactoryShape(shape, target);
    });
  if (from.type === 'object' && to.type === 'object')
    return (
      Object.keys(from.fields).every((key) => Object.hasOwn(to.fields, key)) &&
      Object.entries(to.fields).every(([key, field]) => {
        const source = Object.hasOwn(from.fields, key) ? from.fields[key] : undefined;
        return source
          ? (!field.required || source.required) &&
              compatibleFactoryShape(source.value, field.value)
          : !field.required;
      })
    );
  return false;
}
