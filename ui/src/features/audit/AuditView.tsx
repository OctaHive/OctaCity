import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { Filter, RefreshCw, ShieldCheck } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { formatEnumLabel, formatTimestamp } from '../../shared/display';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { AuditApi, AuditFactPage, AuditFilters } from './api';
import {
  AUDIT_ACTOR_KINDS,
  AUDIT_TEXT_FILTERS,
  auditFiltersFromForm,
  auditTimeInputValue,
  encodeAuditFilters,
  hasAuditFilters,
  readAuditFilters,
  withoutAuditFilters,
} from './auditFilters';
import styles from './AuditView.module.css';

/** Owns the URL-backed Audit filter controls rendered in the contextual tool panel. */
export function AuditFilterPanel() {
  const { t } = usePresentation();
  const [parameters, setParameters] = useSearchParams();
  const [formError, setFormError] = useState<{ message: string; urlState: string } | null>(null);
  const urlState = parameters.toString();

  function apply(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const next = auditFiltersFromForm(new FormData(event.currentTarget), t);
    if (next.error !== null) {
      setFormError({ message: next.error, urlState });
      return;
    }
    setFormError(null);
    setParameters(encodeAuditFilters(parameters, next.filters));
  }

  function clear() {
    setFormError(null);
    setParameters(withoutAuditFilters(parameters));
  }

  const visibleFormError = formError?.urlState === urlState ? formError.message : null;
  return (
    <section aria-labelledby="audit-explorer-filter-heading" className={styles.explorerFilterPanel}>
      <div className={styles.explorerFilterHeading}>
        <div>
          <p className={styles.eyebrow}>{t('audit.exactMatching')}</p>
          <h3 id="audit-explorer-filter-heading">{t('audit.filters')}</h3>
        </div>
        {hasAuditFilters(parameters) ? (
          <button
            aria-label={t('audit.clearFilters')}
            className={styles.textButton}
            onClick={clear}
            type="button"
          >
            {t('audit.clear')}
          </button>
        ) : null}
      </div>
      <p className={styles.filterHelp}>{t('audit.filterHelp')}</p>
      <form
        aria-label={t('audit.filters')}
        className={styles.explorerFilters}
        key={urlState}
        onSubmit={apply}
      >
        <label>
          <span>{t('audit.actorKind')}</span>
          <select defaultValue={parameters.get('actor_kind') ?? ''} name="actor_kind">
            <option value="">{t('audit.allActorKinds')}</option>
            {AUDIT_ACTOR_KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {formatEnumLabel(kind, t)}
              </option>
            ))}
          </select>
        </label>
        {AUDIT_TEXT_FILTERS.map(({ label, maximumBytes, name }) => (
          <FilterField
            defaultValue={parameters.get(name) ?? ''}
            key={name}
            label={t(label)}
            maximumLength={maximumBytes}
            name={name}
          />
        ))}
        <label>
          <span>{t('audit.occurredFrom')}</span>
          <input
            defaultValue={auditTimeInputValue(parameters.get('occurred_from_unix_ms'))}
            name="occurred_from"
            step="0.001"
            type="datetime-local"
          />
        </label>
        <label>
          <span>{t('audit.occurredThrough')}</span>
          <input
            defaultValue={auditTimeInputValue(parameters.get('occurred_through_unix_ms'))}
            name="occurred_through"
            step="0.001"
            type="datetime-local"
          />
        </label>
        <button aria-label={t('audit.applyFilters')} className={styles.applyButton} type="submit">
          <Filter aria-hidden="true" size={15} />
          {t('audit.applyFilters')}
        </button>
      </form>
      {visibleFormError === null ? null : (
        <p className={styles.explorerValidation} role="alert">
          {visibleFormError}
        </p>
      )}
    </section>
  );
}

/** Lists immutable audit evidence without interpreting it as current authorization state. */
export function AuditView({ api }: { api: AuditApi }) {
  const { locale, t } = usePresentation();
  const [parameters] = useSearchParams();
  const parsed = readAuditFilters(parameters, t);
  const audit = useInfiniteQuery<
    AuditFactPage,
    Error,
    InfiniteData<AuditFactPage>,
    ReturnType<typeof queryKeys.auditFacts>,
    string | null
  >({
    enabled: parsed.error === null,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => api.listAuditFacts(parsed.filters, pageParam, signal),
    queryKey: queryKeys.auditFacts(parsed.filters),
  });
  const facts = audit.data?.pages.flatMap((page) => page.items) ?? [];
  const activeFilters = auditFilterLabels(parsed.filters, locale, t);

  return (
    <div className={styles.page}>
      <header className={styles.heading}>
        <div>
          <p className={styles.eyebrow}>{t('audit.immutableEvidence')}</p>
          <h1>{t('section.audit')}</h1>
          <p>{t('audit.description')}</p>
        </div>
        <button
          className={styles.refreshButton}
          disabled={audit.isFetching || parsed.error !== null}
          onClick={() => void audit.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={16} />
          {audit.isFetching && !audit.isFetchingNextPage
            ? t('common.refreshing')
            : t('common.refresh')}
        </button>
      </header>

      <ActiveAuditFilters labels={activeFilters} />

      <section aria-labelledby="audit-results-heading" className={styles.panel}>
        <div className={styles.panelHeading}>
          <div>
            <p className={styles.eyebrow}>{t('audit.newestFirst')}</p>
            <h2 id="audit-results-heading">{t('audit.facts')}</h2>
          </div>
          {audit.data === undefined ? null : (
            <span>{t('audit.loaded', { count: facts.length })}</span>
          )}
        </div>
        <div className={styles.policyNote}>
          <ShieldCheck aria-hidden="true" size={17} />
          <span>{t('audit.policyNote')}</span>
        </div>
        {parsed.error === null ? null : (
          <p className={styles.validation} role="alert">
            {parsed.error}
          </p>
        )}
        {parsed.error !== null ? null : audit.isPending ? (
          <QueryLoadingNotice className={styles.state} label={t('audit.factsLower')} />
        ) : audit.data === undefined ? (
          <QueryFailureNotice
            className={styles.resultNotice}
            error={audit.error}
            onRetry={audit.refetch}
            title={t('audit.loadFailure')}
          />
        ) : (
          <>
            <QueryBackgroundNotice
              className={styles.resultNotice}
              error={audit.error}
              fetching={audit.isFetching && !audit.isFetchingNextPage}
              label={t('audit.factsLower')}
              onRetry={audit.refetch}
            />
            {facts.length === 0 ? (
              <QueryEmptyNotice className={styles.empty}>{t('audit.noMatches')}</QueryEmptyNotice>
            ) : (
              <AuditTable facts={facts} />
            )}
            {audit.hasNextPage ? (
              <button
                className={styles.loadMoreButton}
                disabled={audit.isFetchingNextPage}
                onClick={() => void audit.fetchNextPage()}
                type="button"
              >
                {audit.isFetchingNextPage ? t('audit.loadingFacts') : t('audit.loadMore')}
              </button>
            ) : null}
          </>
        )}
      </section>
    </div>
  );
}

function ActiveAuditFilters({ labels }: { labels: readonly string[] }) {
  const { t } = usePresentation();
  return (
    <aside aria-label={t('audit.activeFilters')} className={styles.activeFilters}>
      <strong>
        {labels.length === 0
          ? t('audit.allFacts')
          : t('audit.activeFilterCount', { count: labels.length })}
      </strong>
      {labels.length === 0 ? (
        <span>{t('audit.noFilters')}</span>
      ) : (
        <ul>
          {labels.map((label) => (
            <li key={label} title={label}>
              {label}
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}

function auditFilterLabels(
  filters: AuditFilters,
  locale: string,
  t: ReturnType<typeof usePresentation>['t'],
): string[] {
  const values: Array<[string, string | number | null]> = [
    [
      t('audit.actorKind'),
      filters.actorKind === null ? null : formatEnumLabel(filters.actorKind, t),
    ],
    [t('audit.actor'), filters.actorIdentity],
    [t('audit.operation'), filters.operation],
    [t('audit.targetKind'), filters.targetKind],
    [t('audit.target'), filters.targetIdentity],
    [t('audit.request'), filters.requestIdentity],
    [
      t('audit.from'),
      filters.occurredFromUnixMs === null
        ? null
        : formatTimestamp(filters.occurredFromUnixMs, locale).display,
    ],
    [
      t('audit.through'),
      filters.occurredThroughUnixMs === null
        ? null
        : formatTimestamp(filters.occurredThroughUnixMs, locale).display,
    ],
  ];
  return values.flatMap(([label, value]) => (value === null ? [] : [`${label}: ${value}`]));
}

function FilterField({
  defaultValue,
  label,
  maximumLength,
  name,
}: {
  defaultValue: string;
  label: string;
  maximumLength: number;
  name: (typeof AUDIT_TEXT_FILTERS)[number]['name'];
}) {
  return (
    <label>
      <span>{label}</span>
      <input defaultValue={defaultValue} maxLength={maximumLength} name={name} type="text" />
    </label>
  );
}

function AuditTable({ facts }: { facts: AuditFactPage['items'] }) {
  const { locale, t } = usePresentation();
  return (
    <div className={styles.tableFrame}>
      <table aria-label={t('audit.immutableFacts')} className={styles.table}>
        <thead>
          <tr>
            <th scope="col">{t('audit.occurred')}</th>
            <th scope="col">{t('audit.actor')}</th>
            <th scope="col">{t('audit.operation')}</th>
            <th scope="col">{t('audit.target')}</th>
            <th scope="col">{t('audit.requestIdentity')}</th>
            <th scope="col">{t('audit.outcome')}</th>
            <th scope="col">{t('audit.evidence')}</th>
          </tr>
        </thead>
        <tbody>
          {facts.map((fact) => {
            const occurred = formatTimestamp(fact.occurred_at_unix_ms, locale);
            return (
              <tr key={fact.id}>
                <td>
                  <time dateTime={occurred.machine ?? undefined}>{occurred.display}</time>
                </td>
                <td>
                  <strong>{formatEnumLabel(fact.actor.kind, t)}</strong>
                  <span>{fact.actor.identity ?? t('audit.noActorIdentity')}</span>
                </td>
                <td>{fact.operation}</td>
                <td>
                  <strong>{fact.target_kind}</strong>
                  <span>{fact.target_identity}</span>
                </td>
                <td>{fact.request_identity ?? t('audit.notPublished')}</td>
                <td>
                  <span className={styles.outcome}>{formatEnumLabel(fact.outcome, t)}</span>
                </td>
                <td>
                  <details className={styles.evidence}>
                    <summary>{t('audit.viewDetails')}</summary>
                    <dl>
                      <dt>{t('audit.factIdentity')}</dt>
                      <dd>{fact.id}</dd>
                      <dt>{t('audit.idempotencyKey')}</dt>
                      <dd>{fact.idempotency_key ?? t('audit.notPublished')}</dd>
                      <dt>{t('audit.metadata')}</dt>
                      <dd>
                        <pre>{JSON.stringify(fact.metadata, null, 2)}</pre>
                      </dd>
                    </dl>
                  </details>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
