import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { compatibleFactoryShape } from './validateDraft';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Shape = Schema['FactoryFlowValueSchema'];
type Mapping = Schema['FactoryFlowValueMapping'];
type Field = { parts: string[]; shape: Shape };
const pointer = (parts: string[]) =>
  parts.map((part) => `/${part.replaceAll('~', '~0').replaceAll('/', '~1')}`).join('');
function fields(shape: Shape, parts: string[] = []): Field[] {
  if (parts.length > FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueSchema['x-max-depth']) return [];
  return shape.type === 'object'
    ? Object.entries(shape.fields).flatMap(([key, field]) => fields(field.value, [...parts, key]))
    : [{ parts, shape }];
}
function insert(mapping: Mapping, parts: string[], source: string): Mapping | null {
  const [first, ...rest] = parts;
  if (first === undefined) return { mapping: 'pointer', path: `/${source}` };
  if (mapping.mapping !== 'object') return null;
  if (!rest.length && Object.hasOwn(mapping.fields, first)) return null;
  const child = insert(mapping.fields[first] ?? { mapping: 'object', fields: {} }, rest, source);
  return child ? { ...mapping, fields: { ...mapping.fields, [first]: child } } : null;
}

function aliasAt(path: string) {
  return path.split('/')[1]?.replaceAll('~1', '/').replaceAll('~0', '~');
}
function uses(mapping: Mapping, alias: string): boolean {
  switch (mapping.mapping) {
    case 'pointer':
      return aliasAt(mapping.path) === alias;
    case 'object':
      return Object.values(mapping.fields).some((value) => uses(value, alias));
    case 'array':
      return mapping.items.some((value) => uses(value, alias));
    case 'if_present':
      return (
        aliasAt(mapping.path) === alias ||
        uses(mapping.then, alias) ||
        uses(mapping.otherwise, alias)
      );
    case 'constant':
      return false;
  }
}
function disconnect(mapping: Mapping, parts: string[]): Mapping {
  const [first, ...rest] = parts;
  if (first === undefined) return { mapping: 'object', fields: {} };
  if (mapping.mapping !== 'object') return mapping;
  const next = { ...mapping.fields };
  if (!rest.length) delete next[first];
  else if (Object.hasOwn(next, first)) {
    const child = disconnect(next[first]!, rest);
    if (child.mapping === 'object' && !Object.keys(child.fields).length) delete next[first];
    else next[first] = child;
  }
  return { ...mapping, fields: next };
}
function connections(
  mapping: Mapping,
  sources: Schema['FactoryFlowInputBinding']['sources'],
  parts: string[] = [],
): {
  parts: string[];
  alias: string;
  source: Extract<Schema['FactoryFlowInputSource'], { source: 'record' }>;
}[] {
  if (mapping.mapping === 'object')
    return Object.entries(mapping.fields).flatMap(([key, value]) =>
      connections(value, sources, [...parts, key]),
    );
  if (mapping.mapping !== 'pointer' || !/^\/[^/]+$/.test(mapping.path)) return [];
  const alias = aliasAt(mapping.path)!;
  const source = Object.hasOwn(sources, alias) ? sources[alias] : undefined;
  return source?.source === 'record' && (!source.view || source.view === 'payload')
    ? [{ parts, alias, source }]
    : [];
}

/** Explicit public result-field bindings reuse the ordinary record selector, without copying a producer's input or Context. */
export function AcceptedResultFields({
  node,
  root,
  schemas,
  onChange,
}: {
  node: Node;
  root: Schema['FactoryFlowRootDraft'];
  schemas: readonly Schema['FactoryFlowDataSchema'][];
  onChange: (node: Node) => void;
}) {
  const { t } = usePresentation();
  const [producerKey, setProducerKey] = useState('');
  const [outcomeKey, setOutcomeKey] = useState('');
  const [resultPath, setResultPath] = useState<string | null>(null);
  const [inputPath, setInputPath] = useState<string | null>(null);
  const producer = root.nodes.find((item) => item.key === producerKey && item !== node);
  const outcome =
    producer?.outcomes.find((item) => item.key === outcomeKey) ??
    (producer?.outcomes.length === 1 ? producer.outcomes[0] : undefined);
  const shape = (reference: Node['input_schema']) =>
    schemas.find(
      (schema) =>
        reference &&
        schema.reference.identity === reference.identity &&
        schema.reference.version === reference.version &&
        schema.reference.digest === reference.digest,
    )?.shape;
  const resultShape = shape(outcome?.schema ?? null);
  const inputShape = shape(node.input_schema);
  const resultFields = resultShape ? fields(resultShape) : [];
  const selectedResult = resultFields.find((field) => pointer(field.parts) === resultPath);
  const inputFields = inputShape
    ? fields(inputShape).filter(
        (field) => selectedResult && compatibleFactoryShape(selectedResult.shape, field.shape),
      )
    : [];
  const selectedInput = inputFields.find((field) => pointer(field.parts) === inputPath);
  const sources = node.input_binding?.sources ?? {};
  const maximum =
    FACTORY_AUTHORING_SCHEMAS.FactoryFlowInputBinding.properties.sources.maxProperties;
  const alias = Array.from({ length: maximum }, (_, index) => `accepted_${index}`).find(
    (key) => !Object.hasOwn(sources, key),
  );
  const mapping = node.input_binding?.mapping.value ?? { mapping: 'object' as const, fields: {} };
  const wholeInputOccupied =
    selectedInput?.parts.length === 0 &&
    node.input_binding &&
    !(mapping.mapping === 'object' && Object.keys(mapping.fields).length === 0);
  const nextMapping =
    selectedInput && alias && !wholeInputOccupied
      ? insert(mapping, selectedInput.parts, alias)
      : null;
  const options = (items: Field[]) =>
    items.map((field) => ({
      value: pointer(field.parts),
      label: field.parts.join(' › ') || t('factory.entireTypedValue'),
    }));
  return (
    <fieldset>
      <legend>{t('factory.acceptedResults')}</legend>
      <p>{t('factory.acceptedResultsHelp')}</p>
      <ul>
        {connections(mapping, sources).map((connection) => {
          const input = connection.parts.join(' › ') || t('factory.entireTypedValue');
          const field =
            connection.source.path
              .split('/')
              .slice(1)
              .map((part) => part.replaceAll('~1', '/').replaceAll('~0', '~'))
              .join(' › ') || t('factory.entireTypedValue');
          return (
            <li key={pointer(connection.parts)}>
              <span>
                {t('factory.acceptedFieldSummary', {
                  input,
                  step: connection.source.node,
                  outcome: connection.source.outcome,
                  field,
                })}
              </span>
              <button
                type="button"
                onClick={() => {
                  const value = disconnect(mapping, connection.parts);
                  onChange({
                    ...node,
                    input_binding: {
                      sources: uses(value, connection.alias)
                        ? sources
                        : Object.fromEntries(
                            Object.entries(sources).filter(([key]) => key !== connection.alias),
                          ),
                      mapping: { value },
                    },
                  });
                  setResultPath(null);
                  setInputPath(null);
                }}
              >
                {t('factory.disconnectAcceptedField', { name: input })}
              </button>
            </li>
          );
        })}
      </ul>

      {!inputShape ? <p>{t('factory.selectInputSchemaFirst')}</p> : null}
      <SelectMenu
        ariaLabel={t('factory.resultFromStep')}
        value={producerKey}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...root.nodes
            .filter((item) => item !== node && !!item.key)
            .map((item) => ({ value: item.key, label: item.key })),
        ]}
        onValueChange={(value) => {
          setProducerKey(value);
          setOutcomeKey('');
          setResultPath(null);
          setInputPath(null);
        }}
      />
      {producer && producer.outcomes.length > 1 ? (
        <SelectMenu
          ariaLabel={t('factory.transitionOutcome')}
          value={outcome?.key ?? ''}
          options={[
            { value: '', label: t('factory.chooseInput') },
            ...producer.outcomes.map((item) => ({ value: item.key, label: item.key })),
          ]}
          onValueChange={(value) => {
            setOutcomeKey(value);
            setResultPath(null);
            setInputPath(null);
          }}
        />
      ) : null}
      <SelectMenu
        ariaLabel={t('factory.resultField')}
        value={resultPath ?? 'unselected'}
        options={[
          { value: 'unselected', label: t('factory.chooseInput') },
          ...options(resultFields),
        ]}
        onValueChange={(value) => {
          setResultPath(value === 'unselected' ? null : value);
          setInputPath(null);
        }}
      />
      <SelectMenu
        ariaLabel={t('factory.stepInputField')}
        value={inputPath ?? 'unselected'}
        options={[
          { value: 'unselected', label: t('factory.chooseInput') },
          ...options(inputFields),
        ]}
        onValueChange={(value) => setInputPath(value === 'unselected' ? null : value)}
      />
      {selectedResult && inputShape && !inputFields.length ? (
        <p role="alert">{t('factory.noCompatibleInputField')}</p>
      ) : null}
      {Object.keys(sources).length >= maximum ? (
        <p role="alert">{t('factory.inputSourceLimitReached')}</p>
      ) : null}
      {selectedInput && alias && !nextMapping ? (
        <p role="alert">{t('factory.inputFieldAlreadyMapped')}</p>
      ) : null}
      <button
        type="button"
        disabled={
          !producer ||
          !outcome ||
          !selectedResult ||
          !selectedInput ||
          !nextMapping ||
          !alias ||
          Object.keys(sources).length >= maximum
        }
        onClick={() => {
          if (producer && outcome && selectedResult && nextMapping && alias)
            onChange({
              ...node,
              input_binding: {
                sources: {
                  ...sources,
                  [alias]: {
                    source: 'record',
                    node: producer.key,
                    outcome: outcome.key,
                    view: 'payload',
                    path: pointer(selectedResult.parts),
                  },
                },
                mapping: { value: nextMapping },
              },
            });
          setResultPath(null);
          setInputPath(null);
        }}
      >
        {t('factory.useAcceptedField')}
      </button>
    </fieldset>
  );
}
