import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import { useEffect, useRef, useState } from 'react';
import { replaceEqualDeep } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { TypedIssueClassification } from './TypedIssueClassification';
import {
  classificationAction,
  assessmentMapping,
  classificationParameters,
  matchesClassification,
  sameClassificationProducer,
  withoutClassification,
  type ClassificationChoices,
} from './issueClassification';

type S = components['schemas'];
type Node = S['FactoryFlowNodeDefinition'];
type Schema = S['FactoryFlowDataSchema'];

export function ClassificationEffects({ choices }: { choices: ClassificationChoices }) {
  const { t } = usePresentation();
  return (
    <>
      <p>{t('factory.classificationPriority', { max: choices.priority_maximum })}</p>
      <dl>
        {choices.size_labels.map(({ size, label, name }) => (
          <div key={size}>
            <dt>{size}</dt>
            <dd title={label}>{name}</dd>
          </div>
        ))}
      </dl>
    </>
  );
}

/** Configures a local action with one accepted predecessor and the installed Project mapping. */
export function FlowIssueClassification({
  node,
  root,
  schemas,
  services,
  onChange,
  onPendingChange,
  onRemove,
}: {
  node: Node;
  root: S['FactoryFlowRootDraft'];
  schemas: readonly Schema[];
  services: FlowAuthoringServices;
  onChange: (node: Node, schemas?: Schema[]) => void;
  onRemove?: ((root: S['FactoryFlowRootDraft']) => void) | undefined;
  onPendingChange?: ((pending: boolean) => void) | undefined;
}) {
  const { t } = usePresentation();
  const configured = node.action?.action === classificationAction;
  const [enabled, setEnabled] = useState(false);
  const [mode, setMode] = useState(() =>
    configured && !classificationParameters(node.action)?.fields ? 'legacy' : 'project',
  );
  const [source, setSource] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(
    () => () => {
      if (requestRef.current) {
        requestRef.current.abort();
        onPendingChange?.(false);
      }
    },
    [onPendingChange],
  );
  const capabilities = useFactoryAuthoringCapabilities(services, enabled || configured, 0);
  const offered = capabilities.data?.issue_classification;
  const choices = offered?.size_labels.length ? offered : undefined;
  const issueFields = capabilities.data?.issue_fields;
  const typed = mode === 'project' && !!offered && !!issueFields?.classification;
  const producers = root.transitions.flatMap((edge) => {
    if (
      edge.target.kind !== 'node' ||
      edge.target.key !== node.key ||
      edge.predecessor === node.key
    )
      return [];
    const outcome = root.nodes
      .find(({ key }) => key === edge.predecessor)
      ?.outcomes.find(({ key, kind }) => key === edge.outcome && kind === 'success');
    if (!outcome) return [];
    const shape = schemas.find(
      ({ reference }) => sourceReferenceKey(reference) === sourceReferenceKey(outcome.schema),
    )?.shape;
    if (shape?.type !== 'object') return [];
    const fields = Object.fromEntries(
      ['priority', 'size'].flatMap((key) => {
        const field = shape.fields[key];
        return field ? [[key, field]] : [];
      }),
    );
    if (!Object.keys(fields).length) return [];
    return [
      { key: `${edge.predecessor} › ${outcome.key}`, node: edge.predecessor, outcome, fields },
    ];
  });
  const producer =
    producers.length === 1 ? producers[0] : producers.find(({ key }) => key === source);
  const input: Node['input_binding'] = producer
    ? {
        sources: {
          assessment: {
            source: 'record',
            node: producer.node,
            outcome: producer.outcome.key,
            view: 'payload',
            path: '',
          },
        },
        mapping: { value: assessmentMapping(producer.fields) },
      }
    : null;
  const inputShape = producer && { type: 'object' as const, fields: producer.fields };
  const receipt = node.outcomes.find(({ key }) => key === 'applied');
  const receiptSchema =
    receipt &&
    schemas.find(
      ({ reference }) => sourceReferenceKey(reference) === sourceReferenceKey(receipt.schema),
    );
  const compatibleReceipt =
    !receipt ||
    (receipt.kind === 'success' &&
      !!receiptSchema &&
      replaceEqualDeep(choices?.receipt_schema.shape, receiptSchema.shape) ===
        choices?.receipt_schema.shape);
  const conflict =
    !compatibleReceipt ||
    (!!node.input_binding && replaceEqualDeep(node.input_binding, input) !== node.input_binding);
  const latestRef = useRef({ node, producer, receipt, choices });
  latestRef.current = { node, producer, receipt, choices };
  const matches = !!choices && configured && matchesClassification(node.action!, choices);
  const configure = async () => {
    if (
      !choices ||
      !producer ||
      !input ||
      !inputShape ||
      pending ||
      conflict ||
      capabilities.isError
    )
      return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    onPendingChange?.(true);
    setError(null);
    try {
      const normalize = (shape: Schema['shape']) =>
        services.api.normalizeSchema(
          {
            project_id: services.projectId,
            identity: `issue-classification.${crypto.randomUUID()}`,
            version: '1',
            shape,
          },
          controller.signal,
        );
      const existingInput = schemas.find(
        ({ reference, shape }) =>
          node.input_schema &&
          sourceReferenceKey(reference) === sourceReferenceKey(node.input_schema) &&
          replaceEqualDeep(shape, inputShape) === shape,
      );
      const inputSchema = existingInput ?? (await normalize(inputShape));
      const output = receiptSchema ?? choices.receipt_schema;
      if (controller.signal.aborted) return;
      const current = latestRef.current;
      if (
        current.node.kind !== node.kind ||
        current.node.key !== node.key ||
        current.node.action !== node.action ||
        current.node.input_binding !== node.input_binding ||
        current.node.input_schema !== node.input_schema ||
        current.receipt !== receipt ||
        current.choices !== choices ||
        !sameClassificationProducer(current.producer, producer)
      ) {
        throw new Error(t('factory.classificationDraftChanged'));
      }
      const outcome = receipt ?? {
        key: 'applied',
        kind: 'success' as const,
        schema: output.reference,
      };
      const outputBytes =
        output.reference.digest === choices.receipt_schema.reference.digest
          ? choices.receipt_output_bytes
          : 0;
      onChange(
        {
          ...current.node,
          budget: {
            ...current.node.budget,
            max_output_bytes: Math.max(current.node.budget.max_output_bytes, outputBytes),
          },
          input_schema: inputSchema.reference,
          input_binding: input,
          outcomes: receipt ? current.node.outcomes : [...current.node.outcomes, outcome],
          action: {
            action: classificationAction,
            plugin: choices.plugin,
            parameters: {
              source_node: producer.node,
              source_outcome: producer.outcome.key,
              outcome: outcome.key,
              policy: choices.policy,
              size_labels: Object.fromEntries(
                choices.size_labels.map(({ size, label }) => [size, label]),
              ),
            },
          },
        },
        [inputSchema, output],
      );
      setEnabled(false);
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(failure instanceof Error ? failure : new Error('Classification setup failed'));
    } finally {
      if (!controller.signal.aborted) {
        requestRef.current = null;
        setPending(false);
        onPendingChange?.(false);
      }
    }
  };
  const failure = error ?? capabilities.error;
  const withoutWrite = configured && onRemove ? withoutClassification(root, node) : null;
  return (
    <fieldset>
      <legend>{t('factory.localClassification')}</legend>
      <label>
        <input
          type="checkbox"
          checked={enabled || configured}
          disabled={pending || (configured && !withoutWrite)}
          onChange={(event) => {
            setEnabled(event.target.checked);
            if (withoutWrite && !event.target.checked) onRemove?.(withoutWrite);
          }}
        />
        {t('factory.applyIssueClassification')}
      </label>
      {configured ? (
        <p>
          {t(
            withoutWrite
              ? 'factory.classificationDisable'
              : 'factory.classificationDisableDependencies',
          )}
        </p>
      ) : null}
      {enabled || configured ? (
        <>
          {offered && issueFields?.classification && choices ? (
            <SelectMenu
              ariaLabel={t('factory.classificationMode')}
              value={mode}
              options={[
                { value: 'project', label: t('factory.projectClassificationFields') },
                { value: 'legacy', label: t('factory.legacyClassification') },
              ]}
              onValueChange={setMode}
            />
          ) : null}
          {typed && offered && issueFields ? (
            <TypedIssueClassification
              node={node}
              root={root}
              schemas={schemas}
              services={services}
              choices={offered}
              fields={issueFields}
              ready={!capabilities.isFetching && !capabilities.isError}
              onChange={onChange}
              onPendingChange={onPendingChange}
            />
          ) : (
            <>
              <p>{t('factory.classificationHelp')}</p>
              {failure ? (
                <QueryFailureNotice
                  title={t('factory.localClassification')}
                  error={failure}
                  onRetry={() => (error ? void configure() : void capabilities.refetch())}
                />
              ) : null}
              {capabilities.isFetching ? (
                <p role="status">{t('factory.loadingStepCapabilities')}</p>
              ) : null}
              {capabilities.isSuccess && !choices ? (
                <p role="status">{t('factory.classificationSetupNeeded')}</p>
              ) : null}
              {choices && (!configured || matches) ? (
                <ClassificationEffects choices={choices} />
              ) : null}
              {configured && choices && !matches ? (
                <p>{t('factory.classificationMappingChanged')}</p>
              ) : null}
              {!matches && !producers.length ? (
                <p>{t('factory.classificationNeedsSource')}</p>
              ) : null}
              {!matches && producers.length > 1 ? (
                <SelectMenu
                  ariaLabel={t('factory.classificationSource')}
                  value={source}
                  options={[
                    { value: '', label: t('factory.chooseInput') },
                    ...producers.map(({ key }) => ({ value: key, label: key })),
                  ]}
                  onValueChange={setSource}
                />
              ) : null}
              {!matches && conflict ? <p>{t('factory.classificationNeedsEmptyStep')}</p> : null}
              {!matches ? (
                <button
                  type="button"
                  disabled={
                    !producer ||
                    !choices ||
                    !!conflict ||
                    pending ||
                    capabilities.isFetching ||
                    capabilities.isError
                  }
                  onClick={() => void configure()}
                >
                  {t('factory.useProjectClassification')}
                </button>
              ) : null}
            </>
          )}
        </>
      ) : null}
    </fieldset>
  );
}
