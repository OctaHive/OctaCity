import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { SelectMenu } from '../../shared/SelectMenu';
import { ShapeEditor } from './SchemaEditor';
import { publicationChoice, publishCandidateAction } from './PublicationStep';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { FlowSchemaSelect } from './FlowSchemaSelect';
import {
  canSaveIssuePresentation,
  issuePresentationTitle,
  OutcomeIssuePresentationEditor,
  type IssuePresentation,
} from './OutcomeIssuePresentation';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
export type ResultDraft = {
  index: number | null;
  name: string;
  kind: 'success' | 'failure';
  shape: Schema['FactoryFlowValueSchema'];
  issuePresentation?: IssuePresentation | undefined;
};

type Props = {
  node: Node;
  nodes?: readonly Node[];
  schemas: readonly Schema['FactoryFlowDataSchema'][];
  services?: FlowAuthoringServices | undefined;
  draft?: ResultDraft | undefined;
  onDraftChange: (draft: ResultDraft | undefined) => void;
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
};

/** Step-local result editing compiles to the existing server-normalized immutable schema. */
export function StepResults(props: Props) {
  const { node, services } = props;
  if (services && node.kind === 'trusted_action' && node.action?.action === publishCandidateAction)
    return <PublicationResults {...props} services={services} />;
  return <ResultEditor {...props} />;
}

function PublicationResults(props: Props & { services: FlowAuthoringServices }) {
  const { node, services, draft, schemas } = props;
  const { data } = useFactoryAuthoringCapabilities(services);
  const choice = publicationChoice(node.action, data?.candidate_publications ?? []);
  const outcome = draft?.index == null ? undefined : node.outcomes[draft.index];
  const canShowPublication = !!(
    choice?.review_url &&
    outcome &&
    (node.action?.parameters as { require_review_url?: unknown })?.require_review_url === true &&
    draft?.name === choice.outcome.key &&
    draft.kind === choice.outcome.kind &&
    sourceReferenceKey(outcome.schema) === sourceReferenceKey(choice.receipt_schema.reference) &&
    JSON.stringify(draft.shape) ===
      JSON.stringify(
        schemas.find(
          (schema) => sourceReferenceKey(schema.reference) === sourceReferenceKey(outcome.schema),
        )?.shape,
      )
  );
  return <ResultEditor {...props} canShowPublication={canShowPublication} />;
}

function ResultEditor({
  node,
  nodes = [node],
  schemas,
  services,
  draft,
  onDraftChange,
  onChange,
  canShowPublication = false,
}: Props & { canShowPublication?: boolean }) {
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
      issuePresentation: result?.issue_presentation,
      shape: structuredClone(schema?.shape ?? { type: 'string', min_bytes: 0, max_bytes: 4096 }),
    });
  };
  const valid =
    !!draft?.name &&
    new RegExp(FACTORY_AUTHORING_SCHEMAS.FactoryFlowOutcome.properties.key.pattern).test(
      draft.name,
    ) &&
    !node.outcomes.some((item, index) => index !== draft.index && item.key === draft.name) &&
    canSaveIssuePresentation(draft.shape, draft.issuePresentation, canShowPublication);
  const save = async () => {
    if (!services || !draft || !valid || pending) return;
    const controller = new AbortController();
    requestRef.current = controller;
    setPending(true);
    setError(null);
    try {
      const existing = draft.index === null ? undefined : node.outcomes[draft.index];
      const retained = schemas.find((item) => item.reference.digest === existing?.schema.digest);
      // A display-only edit must preserve the schema pinned by the rest of the Flow.
      const schema =
        retained && JSON.stringify(retained.shape) === JSON.stringify(draft.shape)
          ? retained
          : await services.api.normalizeSchema(
              {
                project_id: services.projectId,
                identity: `result.${crypto.randomUUID()}`,
                version: '1',
                shape: draft.shape,
              },
              controller.signal,
            );
      if (controller.signal.aborted) return;
      const outcome: Schema['FactoryFlowOutcome'] = {
        ...existing,
        key: draft.name,
        kind: draft.kind,
        schema: schema.reference,
      };
      if (draft.issuePresentation?.kind === 'custom_v1')
        outcome.issue_presentation = {
          ...draft.issuePresentation,
          title: draft.issuePresentation.title.trim(),
        };
      else if (draft.issuePresentation) outcome.issue_presentation = draft.issuePresentation;
      else delete outcome.issue_presentation;
      onChange(
        {
          outcomes:
            draft.index === null
              ? [...node.outcomes, outcome]
              : node.outcomes.map((old, index) => (index === draft.index ? outcome : old)),
        },
        schema === retained ? [] : [schema],
      );
      onDraftChange(undefined);
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(failure instanceof Error ? failure : new Error('Result validation failed'));
    } finally {
      if (!controller.signal.aborted) setPending(false);
    }
  };
  const failureNotice = error ? (
    <QueryFailureNotice
      title={t('factory.expectedResult')}
      error={error}
      onRetry={draft ? () => void save() : () => setError(null)}
    />
  ) : null;
  return (
    <fieldset>
      <legend>{t('factory.expectedResult')}</legend>
      <p>{t('factory.stepResultHelp')}</p>
      {node.outcomes.map((outcome, index) => {
        const schemaIndex = schemas.findIndex(
          (item) => item.reference.digest === outcome.schema.digest,
        );
        const shape = schemas[schemaIndex]?.shape;
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
            {outcome.issue_presentation ? (
              <p>
                {t('factory.showOnIssue')} · {issuePresentationTitle(outcome.issue_presentation, t)}
              </p>
            ) : null}
            <button type="button" disabled={!!draft || !services} onClick={() => edit(index)}>
              {t('factory.editResult')}
            </button>
            <details>
              <summary>{t('factory.reuseResultFormat')}</summary>
              <FlowSchemaSelect
                schemas={schemas}
                nodes={nodes}
                ariaLabel={t('factory.outcomeSchema')}
                value={String(schemaIndex)}
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
      {!draft && failureNotice}
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
            <OutcomeIssuePresentationEditor
              shape={draft.shape}
              canShowPublication={canShowPublication}
              value={draft.issuePresentation}
              onChange={(issuePresentation) => onDraftChange({ ...draft, issuePresentation })}
            />
            {failureNotice}
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
