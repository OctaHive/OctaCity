import { useState } from 'react';
import type { components } from '../../.generated/api/schema';
import type { ProjectDocumentsApi } from '../api/projectDocuments';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { ProjectDocumentPublication } from './ProjectDocumentPublication';
import { ProjectDocumentReader } from './ProjectDocumentReader';
import { DocumentMarkdown } from './DocumentMarkdown';
import styles from './ProjectDocumentPublisher.module.css';
import { formatTimestamp } from './display';

type Request = components['schemas']['PublishProjectDocumentRequest'];
type LoadedDocument = {
  resource: components['schemas']['ProjectDocumentResource'];
  content: string;
};
interface Props {
  api: ProjectDocumentsApi;
  projectId: string;
}
/** Publishes one reviewed immutable input; assigning it to a stage is a separate explicit choice. */
export function ProjectDocumentPublisher(props: Props) {
  return (
    <div className={styles.workspace}>
      <ProjectDocumentEditor key={props.projectId} {...props} />
    </div>
  );
}
const focusOnMount = (element: HTMLElement | null) => element?.focus();
function ProjectDocumentEditor({ api, projectId }: Props) {
  const { t, locale } = usePresentation();
  const [document, setDocument] = useState<LoadedDocument | null>(null);
  const [editing, setEditing] = useState(false);
  const [reading, setReading] = useState(false);
  if (editing)
    return (
      <DocumentDraft
        api={api}
        projectId={projectId}
        initial={document ? { name: document.resource.name, content: document.content } : undefined}
        onCancel={() => setEditing(false)}
        onSaved={(saved) => {
          setDocument(saved);
          setEditing(false);
        }}
      />
    );
  if (document)
    return (
      <article
        className={`${styles.panel} ${styles.viewer}`}
        aria-label={document.resource.name}
        tabIndex={-1}
        ref={focusOnMount}
      >
        <div className={styles.toolbar}>
          <button className={styles.button} type="button" onClick={() => setDocument(null)}>
            {t('projectDocument.back')}
          </button>
          <button className={styles.button} type="button" onClick={() => setEditing(true)}>
            {t('projectDocument.edit')}
          </button>
        </div>
        <h3>{document.resource.name}</h3>
        <p className={styles.metadata}>
          {formatTimestamp(document.resource.published_at, locale).display}
        </p>
        <DocumentMarkdown content={document.content} />
      </article>
    );
  return (
    <fieldset className={styles.panel} disabled={reading}>
      <legend>{t('projectDocument.manage')}</legend>
      <button
        className={`${styles.button} ${styles.primary}`}
        type="button"
        onClick={() => setEditing(true)}
      >
        {t('projectDocument.new')}
      </button>
      <ProjectDocumentReader
        api={api}
        projectId={projectId}
        onLoading={setReading}
        onLoad={(resource, content) => setDocument({ resource, content })}
      />
    </fieldset>
  );
}
function DocumentDraft({
  api,
  projectId,
  initial,
  onCancel,
  onSaved,
}: Props & {
  initial?: Request | undefined;
  onCancel: () => void;
  onSaved: (document: LoadedDocument) => void;
}) {
  const { t } = usePresentation();
  const [draft, setDraft] = useState<Request>(initial ?? { name: '', content: '' });
  const [preview, setPreview] = useState(false);
  const [published, setPublished] = useState<LoadedDocument | null>(null);
  const [review, setReview] = useState<{ request: Request; returnFocus: HTMLElement } | null>(null);
  return (
    <fieldset className={`${styles.panel} ${styles.editor}`}>
      <legend>{t(initial ? 'projectDocument.edit' : 'projectDocument.new')}</legend>
      <label className={styles.field}>
        {t('projectDocument.name')}
        <input
          className={styles.input}
          ref={focusOnMount}
          value={draft.name}
          required
          onChange={(event) => setDraft({ ...draft, name: event.target.value })}
        />
      </label>
      <div className={styles.toolbar}>
        <button
          className={styles.button}
          type="button"
          aria-pressed={!preview}
          onClick={() => setPreview(false)}
        >
          {t('projectDocument.write')}
        </button>
        <button
          className={styles.button}
          type="button"
          aria-pressed={preview}
          onClick={() => setPreview(true)}
        >
          {t('projectDocument.preview')}
        </button>
      </div>
      {preview ? (
        <section role="region" aria-label={t('projectDocument.previewTitle')}>
          <DocumentMarkdown content={draft.content} />
        </section>
      ) : (
        <label className={styles.field}>
          {t('projectDocument.content')}
          <textarea
            className={styles.input}
            value={draft.content}
            required
            onChange={(event) => setDraft({ ...draft, content: event.target.value })}
          />
        </label>
      )}
      <p className={styles.metadata}>{t('projectDocument.help')}</p>
      <div className={styles.toolbar}>
        <button
          className={`${styles.button} ${styles.primary}`}
          type="button"
          disabled={!draft.name.trim() || !draft.content.trim()}
          onClick={(event) =>
            setReview({ request: { ...draft }, returnFocus: event.currentTarget })
          }
        >
          {t(initial ? 'projectDocument.reviewChanges' : 'projectDocument.publish')}
        </button>
        <button className={styles.button} type="button" onClick={onCancel}>
          {t('projectDocument.cancel')}
        </button>
      </div>
      {review ? (
        <ProjectDocumentPublication
          api={api}
          projectId={projectId}
          request={review.request}
          returnFocus={review.returnFocus}
          onPublished={(resource) => setPublished({ resource, content: review.request.content })}
          onClose={() => (published ? onSaved(published) : setReview(null))}
        />
      ) : null}
    </fieldset>
  );
}
