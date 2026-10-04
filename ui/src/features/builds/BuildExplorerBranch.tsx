import {
  useInfiniteQuery,
  type InfiniteData,
  type QueryKey,
  type UseInfiniteQueryResult,
} from '@tanstack/react-query';
import type { ReactNode } from 'react';

import { ManagementApiError } from '../../api/client';
import styles from './BuildExplorer.module.css';

export interface CursorPage<T> {
  items: T[];
  next_cursor: string | null;
}

interface BranchLabels {
  failure: string;
  loading: string;
  loadMore: string;
  loadingMore: string;
  stale: string;
}

interface PagedBranchProps<T extends { id: string }> {
  children: (items: readonly T[]) => ReactNode;
  className?: string | undefined;
  empty: ReactNode;
  labels: BranchLabels;
  query: UseInfiniteQueryResult<InfiniteData<CursorPage<T>>, Error>;
  revealed?: T | null | undefined;
}

/** Owns the repeated loading, stale, empty, and continuation behavior of one explorer branch. */
export function PagedBranch<T extends { id: string }>({
  children,
  className,
  empty,
  labels,
  query,
  revealed = null,
}: PagedBranchProps<T>) {
  if (query.isPending) return <BranchLoading label={labels.loading} />;
  if (query.data === undefined) {
    return <BranchFailure error={query.error} label={labels.failure} onRetry={query.refetch} />;
  }

  const loaded = query.data.pages.flatMap((page) => page.items);
  const items =
    revealed === null || loaded.some(({ id }) => id === revealed.id)
      ? loaded
      : [...loaded, revealed];

  return (
    <div className={className}>
      {query.error === null ? null : (
        <BranchStale
          label={labels.stale}
          onRetry={query.isFetchNextPageError ? query.fetchNextPage : query.refetch}
        />
      )}
      {items.length === 0 ? <BranchEmpty>{empty}</BranchEmpty> : children(items)}
      {query.hasNextPage ? (
        <LoadMore
          label={query.isFetchingNextPage ? labels.loadingMore : labels.loadMore}
          loading={query.isFetchingNextPage}
          onLoad={query.fetchNextPage}
        />
      ) : null}
    </div>
  );
}

export function useCursorPage<T, TQueryKey extends QueryKey>(
  queryKey: TQueryKey,
  read: (cursor: string | null, signal: AbortSignal) => Promise<CursorPage<T>>,
) {
  return useInfiniteQuery<
    CursorPage<T>,
    Error,
    InfiniteData<CursorPage<T>>,
    TQueryKey,
    string | null
  >({
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => read(pageParam, signal),
    queryKey,
  });
}

export function BranchLoading({ label }: { label: string }) {
  return (
    <p className={styles.branchState} role="status">
      Loading {label}…
    </p>
  );
}

export function BranchFailure({
  error,
  label,
  onRetry,
}: {
  error: Error | null;
  label: string;
  onRetry: () => unknown;
}) {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return (
    <div className={styles.branchFailure} role="alert">
      <span>
        {label} could not be loaded.
        {requestId === null ? null : ` Request ID: ${requestId}`}
      </span>
      <button onClick={() => void onRetry()} type="button">
        Retry
      </button>
    </div>
  );
}

function BranchEmpty({ children }: { children: ReactNode }) {
  return <p className={`${styles.branchState} ${styles.emptyBranch}`}>{children}</p>;
}

export function BranchStale({ label, onRetry }: { label: string; onRetry: () => unknown }) {
  return (
    <div className={styles.branchStale}>
      <span role="status">Refresh failed. Showing loaded {label}.</span>
      <button onClick={() => void onRetry()} type="button">
        Retry
      </button>
    </div>
  );
}

function LoadMore({
  label,
  loading,
  onLoad,
}: {
  label: string;
  loading: boolean;
  onLoad: () => unknown;
}) {
  return (
    <button
      className={styles.loadMore}
      disabled={loading}
      onClick={() => void onLoad()}
      type="button"
    >
      {label}
    </button>
  );
}
