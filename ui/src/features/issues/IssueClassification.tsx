import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import type { Issue, IssuePropertyCatalog, IssuesApi } from './api';

type Field = components['schemas']['IssueClassificationFieldReference'];
function sameField(a: Field, b: Field) {
  return a.kind === b.kind && (a.kind !== 'property' || (b.kind === 'property' && a.id === b.id));
}

/** Applicability is read for this exact Issue revision; outcome recommendations never supply current data. */
export function IssueClassification({
  api,
  issue,
  definitions,
}: {
  api: IssuesApi;
  issue: Issue;
  definitions: IssuePropertyCatalog | undefined;
}) {
  const { t, locale } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issueClassification(issue.project_id, issue.id, issue.version),
    gcTime: 0,
    queryFn: ({ signal }) => api.classification(issue.project_id, issue.id, issue.version, signal),
  });
  const labels = {
    priority: 'issues.priority',
    component: 'classification.component',
    type: 'classification.type',
    size: 'classification.size',
    dependencies: 'classification.dependencies',
  } as const;
  return (
    <section aria-label={t('classification.title')}>
      <h3>{t('classification.title')}</h3>
      <p>{t('classification.help')}</p>
      {query.error ? (
        <QueryFailureNotice
          title={t('classification.title')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : query.isPending || query.isFetching ? (
        <QueryLoadingNotice label={t('classification.title')} />
      ) : query.data.version !== issue.version ? (
        <p role="alert">{t('classification.versionChanged')}</p>
      ) : !query.data.fields.length ? (
        <p>{t('classification.empty')}</p>
      ) : (
        <ul>
          {query.data.fields.map((field) => {
            const reference = field.field;
            const attribution = issue.classification?.fields.find((a) =>
              sameField(a.field, field.field),
            );
            const origin = attribution?.origin;
            const label =
              reference.kind === 'property'
                ? (definitions?.definition.properties.find((p) => p.id === reference.id)?.name ??
                  t('classification.unavailableField'))
                : t(labels[reference.kind]);
            return (
              <li
                key={
                  field.field.kind === 'property' ? `property:${field.field.id}` : field.field.kind
                }
              >
                <strong>{label}</strong>
                <p>
                  {t(
                    field.problems.length ? 'classification.needsReview' : 'classification.current',
                  )}
                </p>
                <p>
                  {origin
                    ? t(`classification.origin.${origin.kind}`)
                    : t('classification.notAssessed')}
                </p>
                {attribution ? (
                  <>
                    <time dateTime={new Date(attribution.assessed_at).toISOString()}>
                      {new Date(attribution.assessed_at).toLocaleString(locale)}
                    </time>{' '}
                    <Link
                      to={`/projects/${encodeURIComponent(issue.project_id)}/issues/${encodeURIComponent(issue.id)}?section=activity&revision=${attribution.assessed_version}`}
                    >
                      {t('classification.assessmentRevision', {
                        version: attribution.assessed_version,
                      })}
                    </Link>
                  </>
                ) : null}
                {origin?.kind === 'pipeline' ? (
                  <p>
                    <Link to={`/factory/${encodeURIComponent(origin.run_id)}`}>
                      {t('classification.producingRun')}
                    </Link>
                  </p>
                ) : null}
                {field.problems.length ? (
                  <ul>
                    {[...new Set(field.problems.map((p) => p.code))].map((code) => (
                      <li key={code}>{t(`classification.problem.${code}`)}</li>
                    ))}
                  </ul>
                ) : null}
                {field.requires_verified_evidence ? (
                  <p>
                    {t(
                      field.problems.length === 0 &&
                        field.evidence.length > 0 &&
                        field.evidence.every((e) => e.problems.length === 0)
                        ? 'classification.verified'
                        : 'classification.verificationRequired',
                    )}
                  </p>
                ) : null}
                {field.evidence.length ? (
                  <details>
                    <summary>
                      {t('classification.evidence', { count: field.evidence.length })}
                    </summary>
                    <ul>
                      {field.evidence.map((e) => (
                        <li key={e.record}>
                          {e.problems.length
                            ? [...new Set(e.problems.map((p) => p.code))].map((code) => (
                                <p key={code}>{t(`classification.problem.${code}`)}</p>
                              ))
                            : t('classification.applicableEvidence')}
                        </li>
                      ))}
                    </ul>
                  </details>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
