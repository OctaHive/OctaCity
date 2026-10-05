import {
  useInfiniteQuery,
  type InfiniteData,
  type QueryKey,
  type UseInfiniteQueryResult,
} from '@tanstack/react-query';
import type { ReactNode } from 'react';

import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from './QueryStateNotice';
import styles from './PagedExplorerBranch.module.css';

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

interface CursorPageOptions {
  enabled?: boolean;
  refetchStaleOnMount?: boolean;
}

/** Owns loading, stale, empty, reveal, and continuation behavior for an explorer branch. */
export function PagedExplorerBranch<T extends { id: string }>({
  children,
  className,
  empty,
  labels,
  query,
  revealed = null,
}: PagedBranchProps<T>) {
  if (query.isPending) return <ExplorerBranchLoading label={labels.loading} />;
  if (query.data === undefined) {
    return (
      <ExplorerBranchFailure error={query.error} label={labels.failure} onRetry={query.refetch} />
    );
  }

  const loaded = query.data.pages.flatMap((page) => page.items);
  const items =
    revealed === null || loaded.some(({ id }) => id === revealed.id)
      ? loaded
      : [...loaded, revealed];

  return (
    <div className={className}>
      <QueryBackgroundNotice
        className={styles.stale}
        error={query.error}
        fetching={query.isFetching && !query.isFetchingNextPage}
        label={labels.stale}
        onRetry={query.isFetchNextPageError ? query.fetchNextPage : query.refetch}
      />
      {items.length === 0 ? (
        <QueryEmptyNotice className={`${styles.state} ${styles.empty}`}>{empty}</QueryEmptyNotice>
      ) : (
        children(items)
      )}
      {query.hasNextPage ? (
        <button
          className={styles.loadMore}
          disabled={query.isFetchingNextPage}
          onClick={() => void query.fetchNextPage()}
          type="button"
        >
          {query.isFetchingNextPage ? labels.loadingMore : labels.loadMore}
        </button>
      ) : null}
    </div>
  );
}

export function useCursorPage<T, TQueryKey extends QueryKey>(
  queryKey: TQueryKey,
  read: (cursor: string | null, signal: AbortSignal) => Promise<CursorPage<T>>,
  { enabled = true, refetchStaleOnMount = true }: CursorPageOptions = {},
) {
  return useInfiniteQuery<
    CursorPage<T>,
    Error,
    InfiniteData<CursorPage<T>>,
    TQueryKey,
    string | null
  >({
    enabled,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => read(pageParam, signal),
    queryKey,
    refetchOnMount: refetchStaleOnMount ? true : (query) => query.state.isInvalidated,
  });
}

export function ExplorerBranchLoading({ label }: { label: string }) {
  return <QueryLoadingNotice className={styles.state} label={label} />;
}

export function ExplorerBranchFailure({
  error,
  label,
  onRetry,
}: {
  error: Error | null;
  label: string;
  onRetry: () => unknown;
}) {
  return (
    <QueryFailureNotice
      className={styles.failure}
      error={error}
      onRetry={onRetry}
      title={`${label} could not be loaded.`}
    />
  );
}

export function ExplorerBranchBackground({
  error,
  fetching,
  label,
  onRetry,
}: {
  error: Error | null;
  fetching: boolean;
  label: string;
  onRetry: () => unknown;
}) {
  return (
    <QueryBackgroundNotice
      className={styles.stale}
      error={error}
      fetching={fetching}
      label={label}
      onRetry={onRetry}
    />
  );
}
