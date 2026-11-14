import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssueMaterialReference } from './documentsApi';

export function materialId(reference: IssueMaterialReference): string {
  return reference.kind === 'comment' ? reference.comment_id : reference.artifact.artifact_id;
}

/** Artifact and comment IDs belong to separate owner namespaces. */
export function materialKey(reference: IssueMaterialReference): string {
  return `${reference.kind}:${reference.project_id}:${'issue_id' in reference ? reference.issue_id : ''}:${materialId(reference)}`;
}

/** Exact metadata only, reusable in the selected list and the frozen command review. */
export function IssueMaterialSummary({ reference }: { reference: IssueMaterialReference }) {
  const { t } = usePresentation();
  return (
    <>
      <p>
        {t(
          reference.kind === 'project_document'
            ? 'issueDocuments.shared'
            : reference.kind === 'comment'
              ? 'issueDocuments.comment'
              : 'issueDocuments.owned',
        )}
      </p>
      <p>
        <code>{reference.project_id}</code>
        {'issue_id' in reference ? (
          <>
            {' '}
            · <code>{reference.issue_id}</code>
          </>
        ) : null}
      </p>
      <p>
        <code>{materialId(reference)}</code> ·{' '}
        <code>
          {reference.kind === 'comment' ? reference.body_digest : reference.artifact.content_digest}
        </code>
        {reference.kind !== 'comment' ? <> · {reference.artifact.encoded_size} B</> : null}
      </p>
    </>
  );
}
