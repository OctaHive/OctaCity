import { useState } from 'react';

import { projectDocumentsApi } from '../../api/projectDocuments';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ProjectDocumentPublisher } from '../../shared/ProjectDocumentPublisher';

/** Project owns immutable document publication; steps select exact inputs separately. */
export function ProjectDocuments({ projectId }: { projectId: string }) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  return (
    <section aria-label={t('projectDocument.manage')}>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
        {t('projectDocument.manage')}
      </button>
      {open ? <ProjectDocumentPublisher api={projectDocumentsApi} projectId={projectId} /> : null}
    </section>
  );
}
