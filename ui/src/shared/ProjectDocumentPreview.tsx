import { useState, type ComponentProps } from 'react';
import { ProjectDocumentRead } from './ProjectDocumentReader';

/** Explicit exact-content reads stay in component memory and never publish or attach inputs. */
export function ProjectDocumentPreview(
  props: Pick<
    ComponentProps<typeof ProjectDocumentRead>,
    'api' | 'projectId' | 'document' | 'label'
  >,
) {
  const [preview, setPreview] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  return (
    <>
      <ProjectDocumentRead
        {...props}
        disabled={reading}
        onLoading={setReading}
        onLoad={(value) => setPreview(value.content)}
      />
      {preview !== null ? <pre>{preview}</pre> : null}
    </>
  );
}
