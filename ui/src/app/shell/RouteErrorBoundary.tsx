import { AlertTriangle, ArrowLeft } from 'lucide-react';
import { Link, isRouteErrorResponse, useRouteError } from 'react-router-dom';

import { CONSOLE_PATHS } from '../routes';
import styles from './Shell.module.css';

export function RouteErrorBoundary() {
  const error = useRouteError();
  const notFound = isRouteErrorResponse(error) && error.status === 404;

  return (
    <main className={styles.standaloneError}>
      <AlertTriangle aria-hidden="true" size={28} />
      <p className={styles.eyebrow}>Route error</p>
      <h1>{notFound ? 'Page not found' : 'This view could not be displayed'}</h1>
      <p>
        {notFound
          ? 'The requested console route does not exist.'
          : 'Return to Projects and retry the operation from a known route.'}
      </p>
      <Link className={styles.returnLink} to={CONSOLE_PATHS.projects}>
        <ArrowLeft aria-hidden="true" size={17} />
        Return to Projects
      </Link>
    </main>
  );
}

export function NotFoundView() {
  return (
    <section className={styles.placeholder} aria-labelledby="page-title">
      <p className={styles.eyebrow}>Not found</p>
      <h1 id="page-title">Unknown console route</h1>
      <p className={styles.placeholderDescription}>
        Check the copied URL or return to the Project hierarchy.
      </p>
      <Link className={styles.returnLink} to={CONSOLE_PATHS.projects}>
        <ArrowLeft aria-hidden="true" size={17} />
        Return to Projects
      </Link>
    </section>
  );
}
