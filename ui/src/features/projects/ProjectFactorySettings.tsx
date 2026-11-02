import { Suspense, useId, useState, type ReactNode } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryLoadingNotice } from '../../shared/QueryStateNotice';
import styles from './Projects.module.css';

/** Load on demand; collapsing settings preserves the mounted in-memory policy draft. */
export function ProjectFactorySettings({
  projectId,
  renderSettings,
}: {
  projectId: string;
  renderSettings: (projectId: string) => ReactNode;
}) {
  const { t } = usePresentation();
  const id = useId();
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState(false);
  return (
    <section>
      <button
        className={styles.secondaryButton}
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setLoaded(true);
          setOpen(!open);
        }}
      >
        {t('policy.settings')}
      </button>
      <div id={id} hidden={!open}>
        {loaded ? (
          <Suspense fallback={<QueryLoadingNotice label={t('policy.settings')} />}>
            {renderSettings(projectId)}
          </Suspense>
        ) : null}
      </div>
    </section>
  );
}
