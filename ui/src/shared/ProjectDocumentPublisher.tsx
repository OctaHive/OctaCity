import { useState } from 'react';
import type { components } from '../../.generated/api/schema';
import type { ProjectDocumentsApi } from '../api/projectDocuments';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { ProjectDocumentPublication } from './ProjectDocumentPublication';
import { ProjectDocumentReader } from './ProjectDocumentReader';

type Request = components['schemas']['PublishProjectDocumentRequest'];
interface Props {
  api: ProjectDocumentsApi;
  projectId: string;
}
/** Publishes one reviewed immutable input; assigning it to a stage is a separate explicit choice. */
export function ProjectDocumentPublisher(props: Props) {
  return <ProjectDocumentEditor key={props.projectId} {...props} />;
}
function ProjectDocumentEditor({ api, projectId }: Props) {
  const { t } = usePresentation();
  const [draft, setDraft] = useState<Request>({ name: '', content: '' });
  const [reading, setReading] = useState(false);
  const [review, setReview] = useState<{ request: Request; returnFocus: HTMLElement } | null>(null);
  return (
    <fieldset disabled={reading}>
      <legend>{t('projectDocument.publish')}</legend>
      <p>{t('projectDocument.help')}</p>
      <ProjectDocumentReader
        api={api}
        projectId={projectId}
        onLoad={setDraft}
        onLoading={setReading}
      />
      <label>
        {t('projectDocument.name')}
        <input
          value={draft.name}
          required
          onChange={(event) => setDraft({ ...draft, name: event.target.value })}
        />
      </label>
      <label>
        {t('projectDocument.content')}
        <textarea
          value={draft.content}
          required
          onChange={(event) => setDraft({ ...draft, content: event.target.value })}
        />
      </label>
      <button
        type="button"
        disabled={!draft.name.trim() || !draft.content.trim()}
        onClick={(event) => setReview({ request: { ...draft }, returnFocus: event.currentTarget })}
      >
        {t('projectDocument.review')}
      </button>
      {review ? (
        <ProjectDocumentPublication
          api={api}
          projectId={projectId}
          request={review.request}
          returnFocus={review.returnFocus}
          onClose={() => setReview(null)}
        />
      ) : null}
    </fieldset>
  );
}
