import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { SelectMenu } from '../../shared/SelectMenu';
import { ShapeEditor } from './SchemaEditor';
import type { FlowAuthoringServices } from './FlowPrimitives';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
export type ResultDraft = {
  index: number | null;
  name: string;
  kind: 'success' | 'failure';
  shape: Schema['FactoryFlowValueSchema'];
};

/** Step-local result editing compiles to the existing server-normalized immutable schema. */
export function StepResults({
  node,
  schemas,
  services,
  draft,
  onDraftChange,
  onChange,
}: {
  node: Node;
  schemas: readonly Schema['FactoryFlowDataSchema'][];
  services?: FlowAuthoringServices | undefined;
  draft?: ResultDraft | undefined;
  onDraftChange: (draft: ResultDraft | undefined) => void;
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
}) {
  const { t } = usePresentation();
  const outcomeKeys = useDraftRowKeys(node.outcomes);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), []);
  const edit = (index: number | null) => {
    setError(null);
    const result = index === null ? undefined : node.outcomes[index];
    const schema = schemas.find((item) => item.reference.digest === result?.schema.digest);
    if (result && !schema) {
      setError(new Error('Result format is unavailable'));
      return;
    }
    onDraftChange({
      index,
      name:
        result?.key ??
        Array.from({ length: node.outcomes.length + 1 }, (_, index) =>
          index ? `result_${index + 1}` : 'result',
        ).find((key) => !node.outcomes.some((item) => item.key === key))!,
      kind: result?.kind ?? 'success',
      shape: structuredClone(schema?.shape ?? { type: 'string', min_bytes: 0, max_bytes: 4096 }),
    });
  };
  const valid =
    !!draft?.name &&
    new RegExp(FACTORY_AUTHORING_SCHEMAS.FactoryFlowOutcome.properties.key.pattern).test(
      draft.name,
    ) &&
    !node.outcomes.some((item, index) => index !== draft.index && item.key === draft.name);
  const save = async () => {
    if (!services || !draft || !valid || pending) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    setError(null);
    try {
      const schema = await services.api.normalizeSchema(
        {
          project_id: services.projectId,
          identity: `result.${crypto.randomUUID()}`,
          version: '1',
          shape: draft.shape,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      const outcome = {
        ...(draft.index === null ? {} : node.outcomes[draft.index]),
        key: draft.name,
        kind: draft.kind,
        schema: schema.reference,
      };
      onChange(
        {
          outcomes:
            draft.index === null
              ? [...node.outcomes, outcome]
              : node.outcomes.map((old, index) => (index === draft.index ? outcome : old)),
        },
        [schema],
      );
      onDraftChange(undefined);
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(failure instanceof Error ? failure : new Error('Result validation failed'));
    } finally {
      if (!controller.signal.aborted) setPending(false);
    }
  };
  return (
    <fieldset>
      <legend>{t('factory.expectedResult')}</legend>
      <p>{t('factory.stepResultHelp')}</p>
      {node.outcomes.map((outcome, index) => {
        const shape = schemas.find(
          (item) => item.reference.digest === outcome.schema.digest,
        )?.shape;
        return (
          <fieldset key={outcomeKeys[index]}>
            <legend>{outcome.key || t('factory.resultName')}</legend>
            <p>{t(outcome.kind === 'success' ? 'factory.success' : 'factory.failure')}</p>
            {shape?.type === 'object' ? (
              <ul>
                {Object.keys(shape.fields).map((name) => (
                  <li key={name}>{name}</li>
                ))}
              </ul>
            ) : shape?.type === 'string' ? (
              <p>{t('factory.textResult')}</p>
            ) : null}
            <button type="button" disabled={!!draft || !services} onClick={() => edit(index)}>
              {t('factory.editResult')}
            </button>
            <details>
              <summary>{t('factory.resultDetails')}</summary>
              <p>
                {outcome.schema.identity} · {outcome.schema.version}
              </p>
              <SelectMenu
                ariaLabel={t('factory.outcomeSchema')}
                value={String(
                  schemas.findIndex((item) => item.reference.digest === outcome.schema.digest),
                )}
                options={schemas.map((schema, position) => ({
                  value: String(position),
                  label: `${schema.reference.identity} · ${schema.reference.version}`,
                }))}
                onValueChange={(selected) => {
                  const schema = schemas[Number(selected)];
                  if (schema)
                    onChange({
                      outcomes: node.outcomes.map((item, position) =>
                        position === index ? { ...item, schema: schema.reference } : item,
                      ),
                    });
                }}
              />
            </details>
          </fieldset>
        );
      })}
      {error && !draft ? (
        <QueryFailureNotice
          title={t('factory.expectedResult')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
      {!draft ? (
        <button type="button" disabled={!services} onClick={() => edit(null)}>
          {t('factory.addResult')}
        </button>
      ) : (
        <>
          <fieldset disabled={pending}>
            <details>
              <summary>{t('factory.resultDetails')}</summary>
              <p>{t('factory.resultKeyHelp')}</p>
              <label>
                {t('factory.resultName')}
                <input
                  value={draft.name}
                  maxLength={FACTORY_AUTHORING_SCHEMAS.FactoryFlowOutcome.properties.key.maxLength}
                  onChange={(event) => onDraftChange({ ...draft, name: event.target.value })}
                />
              </label>
            </details>
            <SelectMenu
              ariaLabel={t('factory.outcomeKind')}
              value={draft.kind}
              options={[
                { value: 'success', label: t('factory.success') },
                { value: 'failure', label: t('factory.failure') },
              ]}
              onValueChange={(kind) => {
                if (kind === 'success' || kind === 'failure') onDraftChange({ ...draft, kind });
              }}
            />
            <details>
              <summary>{t('factory.resultStructure')}</summary>
              <ShapeEditor
                value={draft.shape}
                onChange={(shape) => onDraftChange({ ...draft, shape })}
              />
            </details>
            {error ? (
              <QueryFailureNotice
                title={t('factory.expectedResult')}
                error={error}
                onRetry={() => void save()}
              />
            ) : null}
            <button type="button" disabled={!valid} onClick={() => void save()}>
              {t('factory.saveResult')}
            </button>
          </fieldset>
          <button
            type="button"
            onClick={() => {
              requestRef.current?.abort();
              setPending(false);
              setError(null);
              onDraftChange(undefined);
            }}
          >
            {t('factory.cancelResult')}
          </button>
        </>
      )}
    </fieldset>
  );
}
