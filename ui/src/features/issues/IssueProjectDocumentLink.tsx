import { materialKey } from './IssueMaterialSummary';
import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { projectDocumentsApi } from '../../api/projectDocuments';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ProjectDocumentReader } from '../../shared/ProjectDocumentReader';
import { DocumentMarkdown } from '../../shared/DocumentMarkdown';
import { useDeadline } from '../../shared/useDeadline';
import type { IssueMaterialReference } from './documentsApi';
import styles from '../../shared/ProjectDocumentPublisher.module.css';
type SharedDocument = components['schemas']['ProjectDocumentResource'];
/** Explicit discovery and preview of a shared source; linking never publishes or copies its body. */
export function IssueProjectDocumentLink({
  projectId,
  disabled,
  references,
  onSelect,
}: {
  projectId: string;
  disabled: boolean;
  references: IssueMaterialReference[];
  onSelect: (reference: IssueMaterialReference, trigger: HTMLElement) => void;
}) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const [reading, setReading] = useState(false);
  const [selected, setSelected] = useState<{ document: SharedDocument; content: string } | null>(
    null,
  );

  return (
    <section className={styles.panel} aria-label={t('issueDocuments.link')}>
      <button
        className={styles.button}
        type="button"
        disabled={reading}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {t('issueDocuments.link')}
      </button>
      {open ? (
        <>
          <p>{t('issueDocuments.shared')}</p>
          <ProjectDocumentReader
            api={projectDocumentsApi}
            projectId={projectId}
            disabled={reading}
            onLoading={(pending) => {
              setReading(pending);
              if (pending) setSelected(null);
            }}
            onLoad={(document, content) => setSelected({ document, content })}
          />
          {selected ? (
            <SharedPreview
              key={selected.document.artifact.artifact_id}
              selected={selected}
              disabled={disabled || reading}
              references={references}
              onSelect={onSelect}
            />
          ) : null}
        </>
      ) : null}
    </section>
  );
}

function SharedPreview({
  selected,
  disabled,
  references,
  onSelect,
}: {
  selected: { document: SharedDocument; content: string };
  disabled: boolean;
  references: IssueMaterialReference[];
  onSelect: (reference: IssueMaterialReference, trigger: HTMLElement) => void;
}) {
  const { t } = usePresentation();
  const expired = useDeadline(selected.document.expires_at);
  const reference: IssueMaterialReference = {
    kind: 'project_document',
    project_id: selected.document.project_id,
    artifact: selected.document.artifact,
  };
  const isSelected = references.some((r) => materialKey(r) === materialKey(reference));
  return (
    <article aria-label={selected.document.name}>
      <p>
        <code>{selected.document.artifact.artifact_id}</code> ·{' '}
        <code>{selected.document.artifact.content_digest}</code>
      </p>
      {expired ? (
        <p role="status">{t('projectDocument.expired')}</p>
      ) : (
        <DocumentMarkdown content={selected.content} />
      )}
      {isSelected ? <p>{t('issueDocuments.isSelected')}</p> : null}
      <button
        className={styles.button}
        type="button"
        disabled={disabled || expired || isSelected}
        onClick={(e) => onSelect(reference, e.currentTarget)}
      >
        {t('issueDocuments.linkExact')}
      </button>
    </article>
  );
}
