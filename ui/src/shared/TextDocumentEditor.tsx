import { useState, type ReactNode } from 'react';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { DocumentMarkdown } from './DocumentMarkdown';
import styles from './ProjectDocumentPublisher.module.css';
const focusOnMount = (element: HTMLElement | null) => element?.focus();
export type TextDocumentDraft = { name: string; content: string };
/** Owner-neutral in-memory editing/preview; publication and replay remain with the caller. */
export function TextDocumentEditor({
  title,
  draft,
  onChange,
  help,
  children,
}: {
  title: string;
  draft: TextDocumentDraft;
  onChange: (draft: TextDocumentDraft) => void;
  help: string;
  children: ReactNode;
}) {
  const { t } = usePresentation();
  const [preview, setPreview] = useState(false);
  return (
    <fieldset className={`${styles.panel} ${styles.editor}`}>
      <legend>{title}</legend>
      <label className={styles.field}>
        {t('projectDocument.name')}
        <input
          className={styles.input}
          ref={focusOnMount}
          value={draft.name}
          required
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
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
            onChange={(e) => onChange({ ...draft, content: e.target.value })}
          />
        </label>
      )}
      <p className={styles.metadata}>{help}</p>
      {children}
    </fieldset>
  );
}
