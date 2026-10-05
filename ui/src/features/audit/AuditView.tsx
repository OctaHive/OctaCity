import { useInfiniteQuery, type InfiniteData } from '@tanstack/react-query';
import { Filter, RefreshCw, ShieldCheck } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { useSearchParams } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { formatEnumLabel, formatTimestamp } from '../../shared/display';
import { QueryFailureNotice, StaleQueryNotice } from '../../shared/QueryStateNotice';
import type { AuditApi, AuditFactPage } from './api';
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

/** Lists immutable audit evidence without interpreting it as current authorization state. */
export function AuditView({ api }: { api: AuditApi }) {
  const [parameters, setParameters] = useSearchParams();
  const [formError, setFormError] = useState<{ message: string; urlState: string } | null>(null);
  const urlState = parameters.toString();
  const parsed = readAuditFilters(parameters);
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
  const visibleFormError = formError?.urlState === urlState ? formError.message : null;

  function apply(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const next = auditFiltersFromForm(new FormData(event.currentTarget));
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

  return (
    <div className={styles.page}>
      <header className={styles.heading}>
        <div>
          <p className={styles.eyebrow}>Immutable operational evidence</p>
          <h1>Audit</h1>
          <p>
            Search committed management facts by exact server-published fields and request
            correlation.
          </p>
        </div>
        <button
          className={styles.refreshButton}
          disabled={audit.isFetching || parsed.error !== null}
          onClick={() => void audit.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={16} />
          {audit.isFetching && !audit.isFetchingNextPage ? 'Refreshing' : 'Refresh'}
        </button>
      </header>

      <section aria-labelledby="audit-filter-heading" className={styles.panel}>
        <div className={styles.panelHeading}>
          <div>
            <p className={styles.eyebrow}>Bounded exact matching</p>
            <h2 id="audit-filter-heading">Audit filters</h2>
          </div>
          {hasAuditFilters(parameters) ? (
            <button className={styles.textButton} onClick={clear} type="button">
              Clear audit filters
            </button>
          ) : null}
        </div>
        <form aria-label="Audit filters" className={styles.filters} key={urlState} onSubmit={apply}>
          <label>
            <span>Actor kind</span>
            <select defaultValue={parameters.get('actor_kind') ?? ''} name="actor_kind">
              <option value="">All actor kinds</option>
              {AUDIT_ACTOR_KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {formatEnumLabel(kind)}
                </option>
              ))}
            </select>
          </label>
          {AUDIT_TEXT_FILTERS.map(({ label, maximumBytes, name }) => (
            <FilterField
              defaultValue={parameters.get(name) ?? ''}
              key={name}
              label={label}
              maximumLength={maximumBytes}
              name={name}
            />
          ))}
          <label>
            <span>Occurred from</span>
            <input
              defaultValue={auditTimeInputValue(parameters.get('occurred_from_unix_ms'))}
              name="occurred_from"
              step="0.001"
              type="datetime-local"
            />
          </label>
          <label>
            <span>Occurred through</span>
            <input
              defaultValue={auditTimeInputValue(parameters.get('occurred_through_unix_ms'))}
              name="occurred_through"
              step="0.001"
              type="datetime-local"
            />
          </label>
          <button className={styles.applyButton} type="submit">
            <Filter aria-hidden="true" size={15} />
            Apply audit filters
          </button>
        </form>
        {visibleFormError === null && parsed.error === null ? null : (
          <p className={styles.validation} role="alert">
            {visibleFormError ?? parsed.error}
          </p>
        )}
      </section>

      <section aria-labelledby="audit-results-heading" className={styles.panel}>
        <div className={styles.panelHeading}>
          <div>
            <p className={styles.eyebrow}>Newest first</p>
            <h2 id="audit-results-heading">Audit facts</h2>
          </div>
          {audit.data === undefined ? null : <span>{facts.length} loaded</span>}
        </div>
        <div className={styles.policyNote}>
          <ShieldCheck aria-hidden="true" size={17} />
          <span>
            Audit facts are evidence of committed operations; they do not grant or describe current
            authority.
          </span>
        </div>
        {parsed.error !== null ? null : audit.isPending ? (
          <div className={styles.state} role="status">
            Loading audit facts…
          </div>
        ) : audit.data === undefined ? (
          <QueryFailureNotice
            className={styles.resultNotice}
            error={audit.error}
            onRetry={audit.refetch}
            title="Audit facts could not be loaded."
          />
        ) : (
          <>
            {audit.error === null ? null : (
              <StaleQueryNotice
                className={styles.resultNotice}
                message="Refresh failed. Showing the last loaded audit facts."
                onRetry={audit.refetch}
              />
            )}
            {facts.length === 0 ? (
              <p className={styles.empty}>No audit facts match the selected filters.</p>
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
                {audit.isFetchingNextPage ? 'Loading audit facts' : 'Load more audit facts'}
              </button>
            ) : null}
          </>
        )}
      </section>
    </div>
  );
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
  return (
    <div className={styles.tableFrame}>
      <table aria-label="Immutable audit facts" className={styles.table}>
        <thead>
          <tr>
            <th scope="col">Occurred</th>
            <th scope="col">Actor</th>
            <th scope="col">Operation</th>
            <th scope="col">Target</th>
            <th scope="col">Request identity</th>
            <th scope="col">Outcome</th>
            <th scope="col">Evidence</th>
          </tr>
        </thead>
        <tbody>
          {facts.map((fact) => {
            const occurred = formatTimestamp(fact.occurred_at_unix_ms);
            return (
              <tr key={fact.id}>
                <td>
                  <time dateTime={occurred.machine ?? undefined}>{occurred.display}</time>
                </td>
                <td>
                  <strong>{formatEnumLabel(fact.actor.kind)}</strong>
                  <span>{fact.actor.identity ?? 'No actor identity published'}</span>
                </td>
                <td>{fact.operation}</td>
                <td>
                  <strong>{fact.target_kind}</strong>
                  <span>{fact.target_identity}</span>
                </td>
                <td>{fact.request_identity ?? 'Not published'}</td>
                <td>
                  <span className={styles.outcome}>{formatEnumLabel(fact.outcome)}</span>
                </td>
                <td>
                  <details className={styles.evidence}>
                    <summary>View fact details</summary>
                    <dl>
                      <dt>Fact identity</dt>
                      <dd>{fact.id}</dd>
                      <dt>Idempotency key</dt>
                      <dd>{fact.idempotency_key ?? 'Not published'}</dd>
                      <dt>Metadata</dt>
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
