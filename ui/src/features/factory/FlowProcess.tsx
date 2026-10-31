import { useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ProjectDocumentPublication } from '../../shared/ProjectDocumentPublication';
import type { FlowDocumentServices } from './FlowDocuments';
import { readExactProjectDocument } from '../../api/projectDocuments';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';

type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
type Kind = 'goals' | 'criteria';
type Review = {
  request: components['schemas']['PublishProjectDocumentRequest'];
  returnFocus: HTMLElement;
};

/** Process inputs use the existing Project publication owner; Flow owns their exact meaning and recipients. */
export function FlowProcess({
  value,
  services,
  onChange,
  onEditingChange,
}: {
  value: Draft;
  services: FlowDocumentServices;
  onChange: (value: Draft) => void;
  onEditingChange?: ((editing: boolean) => void) | undefined;
}) {
  const { t } = usePresentation();
  const [editor, setEditor] = useState<{
    kind: Kind;
    content: string;
    loading: boolean;
    error?: Error;
  } | null>(null);
  const readSequenceRef = useRef(0);
  const [review, setReview] = useState<Review | null>(null);
  const [published, setPublished] = useState(false);
  const [previews, setPreviews] = useState<
    Partial<Record<Kind, { digest: string; content: string }>>
  >({});
  const latestRef = useRef({ value, onChange });
  latestRef.current = { value, onChange };
  const label = (kind: Kind) =>
    t(kind === 'goals' ? 'factory.pipelineGoals' : 'factory.completionCriteria');
  async function openEditor(kind: Kind) {
    onEditingChange?.(true);
    const sequence = ++readSequenceRef.current;
    const document = value.root.process?.[kind];
    setPublished(false);
    setEditor({ kind, content: '', loading: !!document });
    if (!document) return;
    try {
      const content = await readExactProjectDocument(
        services.api,
        services.projectId,
        document.artifact,
      );
      if (sequence === readSequenceRef.current) setEditor({ kind, content, loading: false });
    } catch (error) {
      if (sequence === readSequenceRef.current)
        setEditor({
          kind,
          content: '',
          loading: false,
          error: error instanceof Error ? error : new Error('Document unavailable'),
        });
    }
  }
  return (
    <fieldset>
      <legend>{t('factory.processPurpose')}</legend>
      <p>{t('factory.processPurposeHelp')}</p>
      {(['goals', 'criteria'] as const).map((kind) => {
        const document = value.root.process?.[kind];
        const preview = previews[kind];
        return (
          <section key={kind} aria-label={label(kind)}>
            <h4>{label(kind)}</h4>
            <button type="button" disabled={editor !== null} onClick={() => void openEditor(kind)}>
              {t(kind === 'goals' ? 'factory.editPipelineGoals' : 'factory.editCompletionCriteria')}
            </button>
            {document ? (
              <>
                <button
                  type="button"
                  disabled={editor !== null}
                  onClick={() =>
                    onChange({
                      ...value,
                      root: {
                        ...value.root,
                        process: { ...value.root.process, [kind]: undefined },
                      },
                    })
                  }
                >
                  {t(
                    kind === 'goals'
                      ? 'factory.detachPipelineGoals'
                      : 'factory.detachCompletionCriteria',
                  )}
                </button>
                {preview?.digest === document.artifact.content_digest ? (
                  <pre>{preview.content}</pre>
                ) : null}
                <details>
                  <summary>{t('factory.documentInputDetails')}</summary>
                  <dl>
                    <dt>{t('artifacts.artifact')}</dt>
                    <dd>{document.artifact.artifact_id}</dd>
                    <dt>{t('artifacts.contentIdentity')}</dt>
                    <dd>{document.artifact.content_digest}</dd>
                    <dt>{t('artifacts.size')}</dt>
                    <dd>{document.artifact.encoded_size}</dd>
                  </dl>
                </details>
                {value.root.nodes.map((node) => (
                  <label key={node.key}>
                    <input
                      type="checkbox"
                      checked={document.recipients.includes(node.key)}
                      onChange={(event) =>
                        onChange({
                          ...value,
                          root: {
                            ...value.root,
                            process: {
                              ...value.root.process,
                              [kind]: {
                                ...document,
                                recipients: event.target.checked
                                  ? [...document.recipients, node.key]
                                  : document.recipients.filter((key) => key !== node.key),
                              },
                            },
                          },
                        })
                      }
                    />
                    {t(kind === 'goals' ? 'factory.goalsRecipient' : 'factory.criteriaRecipient', {
                      name: node.key,
                    })}
                  </label>
                ))}
              </>
            ) : (
              <p>
                {t(kind === 'goals' ? 'factory.noPipelineGoals' : 'factory.noCompletionCriteria')}
              </p>
            )}
          </section>
        );
      })}
      {editor ? (
        <section aria-label={label(editor.kind)}>
          <button
            type="button"
            disabled={review !== null}
            onClick={() => {
              readSequenceRef.current += 1;
              setEditor(null);
              onEditingChange?.(false);
            }}
          >
            {t('factory.cancelProcessEdit')}
          </button>
          {editor.loading ? <QueryLoadingNotice label={label(editor.kind)} /> : null}
          {editor.error ? (
            <QueryFailureNotice
              error={editor.error}
              title={label(editor.kind)}
              onRetry={() => void openEditor(editor.kind)}
            />
          ) : null}
          <label>
            {label(editor.kind)}
            <textarea
              value={editor.content}
              disabled={editor.loading || !!editor.error}
              onChange={(event) => setEditor({ ...editor, content: event.target.value })}
            />
          </label>
          <button
            type="button"
            disabled={editor.loading || !!editor.error || !editor.content.trim()}
            onClick={(event) =>
              setReview({
                request: {
                  name: t(
                    editor.kind === 'goals'
                      ? 'factory.pipelineGoals'
                      : 'factory.pipelineCriteriaName',
                  ),
                  content: editor.content,
                },
                returnFocus: event.currentTarget,
              })
            }
          >
            {t(
              editor.kind === 'goals'
                ? 'factory.reviewGoalsPublication'
                : 'factory.reviewCriteriaPublication',
            )}
          </button>
        </section>
      ) : null}
      {review && editor ? (
        <ProjectDocumentPublication
          api={services.api}
          projectId={services.projectId}
          request={review.request}
          confirmLabel={t(
            editor.kind === 'goals' ? 'factory.saveGoalsInput' : 'factory.saveCriteriaInput',
          )}
          successMessage={t('factory.processInputSaved')}
          consequence={`${t('factory.processInputPublicationScope')}\n\n${review.request.content}`}
          returnFocus={review.returnFocus}
          onPublished={(result) => {
            const { value, onChange } = latestRef.current;
            const kind = editor.kind;
            onChange({
              ...value,
              root: {
                ...value.root,
                process: {
                  ...value.root.process,
                  [kind]: {
                    artifact: result.artifact,
                    recipients: value.root.process?.[kind]?.recipients ?? [],
                  },
                },
              },
            });
            setPreviews((current) => ({
              ...current,
              [kind]: { digest: result.artifact.content_digest, content: review.request.content },
            }));
            setPublished(true);
          }}
          onClose={() => {
            setReview(null);
            if (published) {
              setEditor(null);
              onEditingChange?.(false);
            }
          }}
        />
      ) : null}
    </fieldset>
  );
}
