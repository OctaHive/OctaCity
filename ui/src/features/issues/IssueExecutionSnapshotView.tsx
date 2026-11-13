import type { components } from '../../../.generated/api/schema';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { issuePath } from '../../app/routes';
import styles from './Issues.module.css';

type Snapshot = components['schemas']['IssueExecutionSnapshot'];
/** Displays only server-resolved, independently authorized data, never live replacements. */
export function IssueExecutionSnapshotView({
  snapshot,
  digest,
}: {
  snapshot: Snapshot;
  digest: string | null;
}) {
  const { t } = usePresentation();
  const publication = snapshot.definitions[String(snapshot.input.definition_version)];
  const catalog =
    publication &&
    ('selected_v1' in publication ? publication.selected_v1 : publication.definition);
  const properties =
    publication &&
    ('selected_v1' in publication
      ? publication.selected_v1.properties
      : publication.definition.properties.map((definition) => ({
          definition,
          classification_slot: Object.entries(publication.definition.classification ?? {}).find(
            ([, id]) => id === definition.id,
          )?.[0],
        })));

  return (
    <section aria-label={t('issues.snapshotPreview')}>
      <h3>{t('issues.snapshotPreview')}</h3>
      <p>
        {t('issues.executionReview', { version: snapshot.version })} · {digest}
      </p>
      <p>{t('issues.snapshotDataHelp')}</p>
      <p>{t('issues.snapshotDefinitions', { version: snapshot.input.definition_version })}</p>
      <p>{t('issues.snapshotRequirements', { version: snapshot.applicability_version })}</p>
      {snapshot.admission_classification ? (
        <p>
          {t('factory.workPriority')}: {snapshot.admission_classification.priority} ·{' '}
          {t('factory.workRisk')}: {snapshot.admission_classification.risk}
        </p>
      ) : null}
      <dl>
        {Object.entries(snapshot.values)
          .filter(([alias]) => !snapshot.input.materials?.[alias])
          .map(([alias, value]) => {
            const field = snapshot.input.fields[alias];
            const source = field?.source;
            const property =
              source?.kind === 'property'
                ? properties?.find((p) => p.definition.id === source.id)?.definition
                : source?.kind === 'classification'
                  ? properties?.find((p) => p.classification_slot === source.slot)?.definition
                  : undefined;
            const choices =
              property?.kind.type === 'choice'
                ? property.kind.choices
                : source?.kind === 'priority'
                  ? catalog?.priorities
                  : source?.kind === 'component'
                    ? catalog?.components
                    : source?.kind === 'type'
                      ? catalog?.types
                      : undefined;
            const choice =
              typeof value === 'string' ? choices?.find((c) => c.id === value) : undefined;
            return (
              <div key={alias}>
                <dt>
                  {alias} ·{' '}
                  {t(field?.required ? 'issues.snapshotRequired' : 'issues.snapshotOptional')}
                </dt>
                <dd>
                  {source ? (
                    <p>
                      {property
                        ? `${property.name} · ${property.id}`
                        : source.kind === 'property'
                          ? source.id
                          : source.kind === 'classification'
                            ? source.slot
                            : source.kind}
                    </p>
                  ) : null}
                  <p>
                    {choice
                      ? `${choice.name} · ${value}`
                      : typeof value === 'object'
                        ? JSON.stringify(value)
                        : String(value)}
                  </p>
                  {source &&
                  source.kind !== 'title' &&
                  source.kind !== 'description' &&
                  source.kind !== 'acceptance' ? (
                    <SnapshotAttribution
                      attribution={snapshot.attribution[alias]}
                      issueUrl={issuePath(snapshot.project_id, snapshot.issue_id)}
                    />
                  ) : null}
                </dd>
              </div>
            );
          })}
      </dl>
      {Object.entries(snapshot.input.materials ?? {}).map(([alias, policy]) => (
        <section key={alias}>
          <h4>
            {t('issues.snapshotMaterialLimits', {
              alias,
              required: t(policy.required ? 'issues.snapshotRequired' : 'issues.snapshotOptional'),
              count: snapshot.materials?.[alias]?.length ?? 0,
              maximum:
                policy.source.kind === 'selected'
                  ? policy.source.max_entries
                  : policy.source.references.length,
              bytes: policy.max_bytes,
            })}
          </h4>
          {policy.source.kind === 'exact' ? (
            <ul aria-label={t('issues.snapshotExactSources')}>
              {policy.source.references.map((reference) => (
                <li key={materialIdentity(reference)}>
                  {reference.kind === 'comment'
                    ? `${reference.comment_id} · ${reference.body_digest}`
                    : `${reference.artifact.artifact_id} · ${reference.artifact.content_digest} · ${reference.artifact.encoded_size}`}
                </li>
              ))}
            </ul>
          ) : null}
          {(snapshot.materials?.[alias] ?? []).map((material, index) => {
            const source = material.source;
            const reference = material.selection.reference;
            const name = source.kind === 'comment' ? source.comment.id : source.document.name;
            const label = `${t(`issues.snapshotSource.${source.kind}`)} · ${name}`;
            const text = snapshot.values[alias];
            return (
              <section
                key={materialIdentity(reference)}
                aria-label={label}
                className={styles.panel}
              >
                <h5>{label}</h5>
                <p>
                  {reference.project_id}
                  {'issue_id' in reference ? ` · ${reference.issue_id}` : ''}
                </p>
                <p>
                  {reference.kind === 'comment'
                    ? `${reference.comment_id} · ${reference.body_digest}`
                    : `${reference.artifact.artifact_id} · ${reference.artifact.content_digest} · ${reference.artifact.encoded_size}`}
                </p>
                <p>
                  {t('issues.snapshotSelectedAt', {
                    at: new Date(material.selection.selected_at).toISOString(),
                    actor:
                      material.selection.selected_by.identity ??
                      material.selection.selected_by.kind,
                  })}
                </p>
                {Array.isArray(text) && typeof text[index] === 'string' ? (
                  <p className={styles.prose}>{text[index]}</p>
                ) : null}
              </section>
            );
          })}
        </section>
      ))}
      {Object.entries(snapshot.omissions).map(([alias, reason]) => (
        <p key={alias}>{t('issues.snapshotOmission', { alias, reason })}</p>
      ))}
    </section>
  );
}

/** Frozen assessment metadata only; current evidence validity belongs to authorized server reads. */
function SnapshotAttribution({
  attribution,
  issueUrl,
}: {
  attribution: components['schemas']['IssueClassificationAttribution'] | undefined;
  issueUrl: string;
}) {
  const { t, locale } = usePresentation();
  const [params] = useSearchParams();
  if (!attribution) return <p>{t('classification.notAssessed')}</p>;
  const revisionSearch = new URLSearchParams(params);
  revisionSearch.set('section', 'activity');
  revisionSearch.set('revision', String(attribution.assessed_version));
  const { field, origin } = attribution;
  const definitionVersion = 'definition_version' in field ? field.definition_version : undefined;
  const evidence = attribution.evidence ?? [];
  return (
    <>
      <p>{t(`classification.origin.${origin.kind}`)}</p>
      <p>{t('issues.snapshotAssessedBy', { actor: origin.actor.identity ?? origin.actor.kind })}</p>
      <time dateTime={new Date(attribution.assessed_at).toISOString()}>
        {new Date(attribution.assessed_at).toLocaleString(locale)}
      </time>
      <p>
        <Link to={{ pathname: issueUrl, search: revisionSearch.toString() }}>
          {t('classification.assessmentRevision', { version: attribution.assessed_version })}
        </Link>
      </p>
      <p>
        {t('issues.snapshotAssessmentRequirements', { version: attribution.requirements_version })}
      </p>
      {definitionVersion !== undefined ? (
        <p>{t('issues.snapshotAssessmentDefinitions', { version: definitionVersion })}</p>
      ) : null}
      {origin.kind === 'pipeline' ? (
        <p>
          <Link to={`/factory/${encodeURIComponent(origin.run_id)}`}>
            {t('classification.producingRun')}
          </Link>{' '}
          · {origin.policy} · {origin.record}
        </p>
      ) : null}
      {origin.kind === 'source' ? (
        <>
          <p>{t('issues.snapshotAssessmentMapping', origin.mapping)}</p>
          <p>{t('issues.snapshotAssessmentObservation', { record: origin.observation })}</p>
        </>
      ) : null}
      {evidence.length ? (
        <>
          <p>{t('issues.snapshotEvidenceHelp')}</p>
          <ul aria-label={t('classification.evidence', { count: evidence.length })}>
            {evidence.map((record) => (
              <li key={record}>{record}</li>
            ))}
          </ul>
        </>
      ) : (
        <p>{t('issues.snapshotNoEvidence')}</p>
      )}
    </>
  );
}

function materialIdentity(reference: components['schemas']['FactoryFlowIssueMaterialReference']) {
  return reference.kind === 'comment'
    ? `${reference.kind}:${reference.comment_id}:${reference.body_digest}`
    : `${reference.kind}:${reference.artifact.artifact_id}:${reference.artifact.content_digest}`;
}
