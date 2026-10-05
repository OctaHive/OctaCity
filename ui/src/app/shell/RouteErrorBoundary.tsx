import { AlertTriangle, ArrowLeft } from 'lucide-react';
import { Link, isRouteErrorResponse, useRouteError } from 'react-router-dom';

import { usePresentation } from '../presentation/PresentationProvider';
import { CONSOLE_PATHS } from '../routes';
import styles from './RouteView.module.css';

export function RouteErrorBoundary() {
  const { t } = usePresentation();
  const error = useRouteError();
  const notFound = isRouteErrorResponse(error) && error.status === 404;

  return (
    <main className={styles.standaloneError}>
      <AlertTriangle aria-hidden="true" size={28} />
      <p className={styles.eyebrow}>{t('route.error')}</p>
      <h1>{notFound ? t('route.pageNotFound') : t('route.displayFailure')}</h1>
      <p>{notFound ? t('route.missingDescription') : t('route.failureDescription')}</p>
      <Link className={styles.returnLink} to={CONSOLE_PATHS.projects}>
        <ArrowLeft aria-hidden="true" size={17} />
        {t('route.returnProjects')}
      </Link>
    </main>
  );
}

export function NotFoundView() {
  const { t } = usePresentation();
  return (
    <section className={styles.placeholder} aria-labelledby="page-title">
      <p className={styles.eyebrow}>{t('route.notFound')}</p>
      <h1 id="page-title">{t('route.unknown')}</h1>
      <p className={styles.placeholderDescription}>{t('route.unknownDescription')}</p>
      <Link className={styles.returnLink} to={CONSOLE_PATHS.projects}>
        <ArrowLeft aria-hidden="true" size={17} />
        {t('route.returnProjects')}
      </Link>
    </section>
  );
}
