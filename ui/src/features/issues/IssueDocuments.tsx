import { IssueMaterialSummary, materialId, materialKey } from './IssueMaterialSummary';
import { projectDocumentsApi, readExactProjectDocument } from '../../api/projectDocuments';
import type { TextDocumentDraft } from '../../shared/TextDocumentEditor';
import { IssueProjectDocumentLink } from './IssueProjectDocumentLink';
import { IssueMaterialReview, type MaterialReview } from './IssueMaterialReview';
import { ISSUE_DOCUMENT_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { IssueDocumentDraft } from './IssueDocumentDraft';
import { useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { DocumentMarkdown } from '../../shared/DocumentMarkdown';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { formatTimestamp, formatEnumLabel } from '../../shared/display';
import { useDeadline } from '../../shared/useDeadline';
import type { Issue } from './api';
import {
  readExactIssueDocument,
  type IssueDocument,
  type IssueDocumentsApi,
  type IssueMaterialReference,
} from './documentsApi';
import styles from '../../shared/ProjectDocumentPublisher.module.css';
type DocumentSource = Exclude<IssueMaterialReference, { kind: 'comment' }> & { name?: string };
/** Metadata is discoverable; private text is read explicitly and retained only in this mounted view. */
export function IssueDocuments({
  issue,
  api,
  active = true,
  readOnly = false,
  ready = true,
  onRefresh = async () => {},
}: {
  issue: Issue;
  api: IssueDocumentsApi;
  active?: boolean;
  readOnly?: boolean;
  ready?: boolean;
  onRefresh?: () => Promise<void>;
}) {
  const { t, locale } = usePresentation();
  const [review, setReview] = useState<MaterialReview | null>(null);
  const [replacing, setReplacing] = useState<IssueMaterialReference | null>(null);
  const references = (issue.materials ?? []).map((entry) => entry.reference);
  const replacementMissing =
    replacing !== null && !references.some((r) => materialKey(r) === materialKey(replacing));
  function select(reference: IssueMaterialReference, returnFocus: HTMLElement) {
    if (replacementMissing) return;
    const next = replacing
      ? references.map((r) => (materialKey(r) === materialKey(replacing) ? reference : r))
      : [...references, reference];
    setReview({ references: next, version: issue.version, returnFocus });
  }
  const [editing, setEditing] = useState<TextDocumentDraft | null>(null);
  const [preview, setPreview] = useState<{ document: IssueDocument; content: string } | null>(null);
  const [sharedPreview, setSharedPreview] = useState<
    (Awaited<ReturnType<typeof readExactProjectDocument>> & { name: string }) | null
  >(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState<{
    document: DocumentSource;
    error: Error;
  } | null>(null);
  const catalog = useCursorPage(
    queryKeys.issueDocuments(issue.project_id, issue.id),
    async (cursor, signal) => {
      const page = await api.list(issue.project_id, issue.id, cursor, signal);
      return {
        ...page,
        items: page.items
          .filter((doc) => doc.project_id === issue.project_id && doc.issue_id === issue.id)
          .map((doc) => ({ ...doc, id: doc.artifact.artifact_id })),
      };
    },
    { enabled: active },
  );
  // The workspace owns the read lifetime even if a selected source row is removed mid-read.
  async function open(document: DocumentSource) {
    setPreview(null);
    setSharedPreview(null);
    setFailed(null);
    setLoading(true);
    try {
      if (document.kind === 'project_document') {
        const body = await readExactProjectDocument(
          projectDocumentsApi,
          document.project_id,
          document.artifact,
        );
        setSharedPreview({ ...body, name: document.name ?? document.artifact.artifact_id });
      } else setPreview(await readExactIssueDocument(api, document));
    } catch (error) {
      setFailed({
        document,
        error: error instanceof Error ? error : new Error('Document unavailable'),
      });
    } finally {
      setLoading(false);
    }
  }
  const label = t('issueDocuments.title');
  const previewVisible = !editing || readOnly;
  return (
    <section className={styles.workspace} aria-label={label}>
      <h2>{label}</h2>
      {readOnly ? (
        <p role="status">{t('issueDocuments.historical', { version: issue.version })}</p>
      ) : null}
      {editing ? (
        // History changes presentation, not the lifetime of this Issue's unpublished intent.
        <div hidden={readOnly}>
          <IssueDocumentDraft
            api={api}
            issue={issue}
            initial={editing}
            ready={!readOnly}
            onClose={() => setEditing(null)}
          />
        </div>
      ) : !readOnly ? (
        <button
          className={styles.button}
          type="button"
          onClick={() => setEditing({ name: '', content: '' })}
        >
          {t('issueDocuments.new')}
        </button>
      ) : null}
      {!readOnly ? (
        <IssueProjectDocumentLink
          references={references}
          projectId={issue.project_id}
          disabled={
            !ready ||
            replacementMissing ||
            (!replacing && references.length >= bounds.materialsMaximumCount)
          }
          onSelect={select}
        />
      ) : null}
      <section aria-label={t('issueDocuments.selected')} className={styles.panel}>
        <h3>{t('issueDocuments.selected')}</h3>
        <p>{t('issueDocuments.selectionHelp')}</p>
        <ul>
          {(issue.materials ?? []).map((entry) => (
            <li key={materialKey(entry.reference)}>
              <IssueMaterialSummary reference={entry.reference} />
              <p>
                {entry.selected_by.identity ?? formatEnumLabel(entry.selected_by.kind, t)} ·{' '}
                {formatTimestamp(entry.selected_at, locale).display}
              </p>
              {entry.reference.kind !== 'comment' ? (
                <button
                  className={styles.button}
                  type="button"
                  disabled={loading}
                  onClick={() => {
                    const r = entry.reference;
                    if (r.kind !== 'comment') void open(r);
                  }}
                >
                  {t('issueDocuments.preview', { name: materialId(entry.reference) })}
                </button>
              ) : null}
              {!readOnly ? (
                <button
                  className={styles.button}
                  type="button"
                  disabled={!ready}
                  onClick={(e) =>
                    setReview({
                      references: references.filter(
                        (r) => materialKey(r) !== materialKey(entry.reference),
                      ),
                      version: issue.version,
                      returnFocus: e.currentTarget,
                    })
                  }
                >
                  {t('issueDocuments.remove', { id: materialId(entry.reference) })}
                </button>
              ) : null}
              {!readOnly && entry.reference.kind !== 'comment' ? (
                <button
                  className={styles.button}
                  type="button"
                  disabled={!ready}
                  onClick={() => setReplacing(entry.reference)}
                >
                  {t('issueDocuments.replace', { id: materialId(entry.reference) })}
                </button>
              ) : null}
            </li>
          ))}
        </ul>
        {replacementMissing ? <p role="alert">{t('issueDocuments.replacementMissing')}</p> : null}
        {replacing ? (
          <p role="status">
            {t('issueDocuments.chooseReplacement', { id: materialId(replacing) })}
            <button className={styles.button} type="button" onClick={() => setReplacing(null)}>
              {t('projectDocument.cancel')}
            </button>
          </p>
        ) : null}
      </section>
      {review ? (
        <IssueMaterialReview
          api={api}
          issue={issue}
          review={review}
          ready={ready && !readOnly}
          onRefresh={onRefresh}
          onClose={() => {
            setReview(null);
            setReplacing(null);
          }}
        />
      ) : null}
      <PagedExplorerBranch
        query={catalog}
        empty={t('issueDocuments.empty')}
        labels={{
          loading: label,
          failure: label,
          stale: label,
          loadingMore: label,
          loadMore: t('issueDocuments.more'),
        }}
      >
        {(documents) => (
          <ul className={styles.list}>
            {documents.map((doc) => (
              <li key={doc.id} className={styles.panel}>
                <h3>{doc.name}</h3>
                <p>{t('issueDocuments.owned')}</p>
                <p className={styles.metadata}>
                  {formatTimestamp(doc.published_at, locale).display}
                </p>
                <p className={styles.metadata}>
                  <code>{doc.artifact.artifact_id}</code> ·{' '}
                  <code>{doc.artifact.content_digest}</code> · {doc.artifact.encoded_size}
                </p>
                <button
                  className={styles.button}
                  type="button"
                  disabled={loading || catalog.isError}
                  onClick={() => void open({ ...doc, kind: 'issue_document' })}
                >
                  {t('issueDocuments.preview', { name: doc.name })}
                </button>
                {readOnly ? null : references.some(
                    (r) => r.kind === 'issue_document' && r.artifact.artifact_id === doc.id,
                  ) ? (
                  <p>{t('issueDocuments.isSelected')}</p>
                ) : (
                  <button
                    className={styles.button}
                    type="button"
                    disabled={
                      !ready ||
                      replacementMissing ||
                      catalog.isError ||
                      (!replacing && references.length >= bounds.materialsMaximumCount)
                    }
                    onClick={(e) =>
                      select(
                        {
                          kind: 'issue_document',
                          project_id: issue.project_id,
                          issue_id: issue.id,
                          artifact: doc.artifact,
                        },
                        e.currentTarget,
                      )
                    }
                  >
                    {t(replacing ? 'issueDocuments.replaceWith' : 'issueDocuments.select', {
                      name: doc.name,
                    })}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </PagedExplorerBranch>
      {failed ? (
        <QueryFailureNotice
          title={t('issueDocuments.preview', {
            name: failed.document.name ?? failed.document.artifact.artifact_id,
          })}
          error={failed.error}
          onRetry={() => void open(failed.document)}
        />
      ) : null}
      {sharedPreview && previewVisible ? (
        <Preview
          key={sharedPreview.artifact.artifact_id}
          document={sharedPreview}
          content={sharedPreview.content}
        />
      ) : null}
      {preview && previewVisible ? (
        <Preview
          key={preview.document.artifact.artifact_id}
          document={preview.document}
          content={preview.content}
          onEdit={
            readOnly
              ? undefined
              : () => setEditing({ name: preview.document.name, content: preview.content })
          }
        />
      ) : null}
    </section>
  );
}
function Preview({
  document,
  content,
  onEdit,
}: {
  document: Pick<IssueDocument, 'name' | 'expires_at'>;
  content: string;
  onEdit?: (() => void) | undefined;
}) {
  const { t } = usePresentation();
  const expired = useDeadline(document.expires_at);
  return (
    <article className={styles.panel} aria-label={document.name}>
      {expired ? (
        <p role="status">{t('projectDocument.expired')}</p>
      ) : (
        <>
          <DocumentMarkdown content={content} />
          {onEdit ? (
            <button className={styles.button} type="button" onClick={onEdit}>
              {t('issueDocuments.version')}
            </button>
          ) : null}
        </>
      )}
    </article>
  );
}
