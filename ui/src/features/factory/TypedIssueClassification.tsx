import { useEffect, useRef, useState } from 'react';
import { replaceEqualDeep } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import {
  classificationFields,
  ClassificationFieldPicker,
  type IssueFieldChoices,
} from './IssueFieldContracts';
import {
  assessmentMapping,
  classificationAction,
  classificationParameters,
  sameClassificationProducer,
  type ClassificationChoices,
} from './issueClassification';
import { compatibleFactoryShape } from './validateDraft';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { equivalentFlowSchema } from './flowSchemas';

type S = components['schemas'];
type Node = S['FactoryFlowNodeDefinition'];
type Fields = Extract<S['FactoryFlowValueSchema'], { type: 'object' }>['fields'];

/** A selected local write uses Project types and an exact accepted record; it grants no extra access. */
export function TypedIssueClassification({
  node,
  root,
  schemas,
  services,
  choices,
  fields,
  ready,
  onChange,
  onPendingChange,
}: {
  node: Node;
  root: S['FactoryFlowRootDraft'];
  schemas: readonly S['FactoryFlowDataSchema'][];
  services: FlowAuthoringServices;
  choices: ClassificationChoices;
  fields: IssueFieldChoices;
  ready: boolean;
  onChange: (node: Node, schemas?: S['FactoryFlowDataSchema'][]) => void;
  onPendingChange?: ((pending: boolean) => void) | undefined;
}) {
  const { t } = usePresentation();
  const targets = classificationFields(fields);
  const [selected, setSelected] = useState<string[]>(() => {
    const original = classificationParameters(node.action)?.fields;
    if (!original || typeof original !== 'object') return [];
    return targets
      .filter(({ name, field }) =>
        field.source.kind === 'classification'
          ? 'properties' in original &&
            !!original.properties &&
            typeof original.properties === 'object' &&
            Object.entries(original.properties).some(
              ([slot, id]) => slot === name && id === field.property_id,
            )
          : Object.entries(original).some(([key, value]) => key === name && value === true),
      )
      .map(({ name }) => name);
  });
  const [source, setSource] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(
    () => () => {
      requestRef.current?.abort();
      onPendingChange?.(false);
    },
    [onPendingChange],
  );
  const producers = root.transitions.flatMap((edge) => {
    if (
      edge.target.kind !== 'node' ||
      edge.target.key !== node.key ||
      edge.predecessor === node.key
    )
      return [];
    const outcome = root.nodes
      .find((n) => n.key === edge.predecessor)
      ?.outcomes.find((o) => o.key === edge.outcome && o.kind === 'success');
    const shape =
      outcome && schemas.find((schema) => schema.reference.digest === outcome.schema.digest)?.shape;
    return outcome && shape?.type === 'object'
      ? [{ key: `${edge.predecessor} › ${outcome.key}`, node: edge.predecessor, outcome, shape }]
      : [];
  });
  const producer = producers.length === 1 ? producers[0] : producers.find((p) => p.key === source);
  const selectedTargets = targets.filter(({ name }) => selected.includes(name));
  const projection: Fields = {};
  const compatible =
    !!producer &&
    selectedTargets.length > 0 &&
    selectedTargets.every(({ name, field }) => {
      const provided = producer.shape.fields[name];
      if (!provided || !compatibleFactoryShape(provided.value, field.shape)) return false;
      projection[name] = { required: provided.required, value: field.shape };
      return true;
    });
  // Declared dependencies remain exact accepted-record data, never a copied claim from the form.
  if (producer?.shape.fields.assessment_inputs)
    projection.assessment_inputs = producer.shape.fields.assessment_inputs;
  let mapping: S['FactoryFlowValueMapping'] | null = null;
  try {
    mapping = assessmentMapping(projection);
  } catch {
    /* Bounded grammar cannot encode this optional projection. */
  }
  const input = producer &&
    mapping && {
      sources: {
        assessment: {
          source: 'record' as const,
          node: producer.node,
          outcome: producer.outcome.key,
          view: 'payload' as const,
          path: '',
        },
      },
      mapping: { value: mapping },
    };
  const parameters = classificationParameters(node.action);
  const sources = Object.values(node.input_binding?.sources ?? {});
  const original = sources[0];
  const managed =
    node.action?.action === classificationAction &&
    !!parameters?.fields &&
    sources.length === 1 &&
    original?.source === 'record' &&
    original.node === parameters.source_node &&
    original.outcome === parameters.source_outcome &&
    original.view === 'payload' &&
    original.path === '';
  const conflict =
    !!node.input_binding &&
    !managed &&
    replaceEqualDeep(node.input_binding, input) !== node.input_binding;
  const latestRef = useRef({ node, producer, fields, choices });
  latestRef.current = { node, producer, fields, choices };
  const apply = async () => {
    if (!compatible || !input || !producer || conflict || !ready || pending) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    onPendingChange?.(true);
    setError(null);
    try {
      const shape: S['FactoryFlowValueSchema'] = { type: 'object', fields: projection };
      const schema =
        equivalentFlowSchema(schemas, shape) ??
        (await services.api.normalizeSchema(
          {
            project_id: services.projectId,
            identity: `issue-classification.${crypto.randomUUID()}`,
            version: '1',
            shape,
          },
          controller.signal,
        ));
      if (controller.signal.aborted) return;
      const current = latestRef.current;
      if (
        current.node !== node ||
        !sameClassificationProducer(current.producer, producer) ||
        current.fields !== fields ||
        current.choices !== choices
      )
        throw new Error(t('factory.classificationDraftChanged'));
      const selection: S['FactoryIssueClassificationFieldSelection'] = {
        definition_version: fields.definition_version,
        priority: selected.includes('priority'),
        component: selected.includes('component'),
        kind: selected.includes('kind'),
        size: selected.includes('size'),
        properties: Object.fromEntries(
          selectedTargets.flatMap(({ name, field }) =>
            field.source.kind === 'classification' && field.property_id
              ? [[name, field.property_id]]
              : [],
          ),
        ),
      };
      const receipt = node.outcomes.find((o) => o.key === 'applied');
      if (
        receipt &&
        (receipt.kind !== 'success' ||
          receipt.schema.digest !== choices.receipt_schema.reference.digest)
      )
        throw new Error(t('factory.classificationNeedsEmptyStep'));
      onChange(
        {
          ...node,
          input_schema: schema.reference,
          input_binding: input,
          budget: {
            ...node.budget,
            max_output_bytes: Math.max(node.budget.max_output_bytes, choices.receipt_output_bytes),
          },
          outcomes: receipt
            ? node.outcomes
            : [
                ...node.outcomes,
                { key: 'applied', kind: 'success', schema: choices.receipt_schema.reference },
              ],
          action: {
            action: classificationAction,
            plugin: choices.plugin,
            parameters: {
              source_node: producer.node,
              source_outcome: producer.outcome.key,
              outcome: 'applied',
              policy: choices.policy,
              fields: selection,
            },
          },
        },
        [schema, choices.receipt_schema],
      );
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(failure instanceof Error ? failure : new Error(String(failure)));
    } finally {
      if (!controller.signal.aborted) {
        requestRef.current = null;
        setPending(false);
        onPendingChange?.(false);
      }
    }
  };
  return (
    <fieldset disabled={pending}>
      <ClassificationFieldPicker choices={fields} selected={selected} onChange={setSelected} />
      <p>{t('factory.typedClassificationHelp')}</p>
      {producers.length > 1 ? (
        <SelectMenu
          ariaLabel={t('factory.classificationSource')}
          value={source}
          options={[
            { value: '', label: t('factory.chooseInput') },
            ...producers.map((p) => ({ value: p.key, label: p.key })),
          ]}
          onValueChange={setSource}
        />
      ) : null}
      {!compatible ? <p>{t('factory.classificationNeedsCompatibleFields')}</p> : null}
      {!mapping ? <p role="alert">{t('factory.projectionTooLarge')}</p> : null}
      {conflict ? <p>{t('factory.classificationNeedsEmptyStep')}</p> : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.localClassification')}
          error={error}
          onRetry={() => void apply()}
        />
      ) : null}
      <button
        type="button"
        disabled={!ready || !compatible || !input || conflict || pending}
        onClick={() => void apply()}
      >
        {t('factory.useProjectClassification')}
      </button>
    </fieldset>
  );
}
