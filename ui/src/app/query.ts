import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
  readiness: ['system', 'readiness'] as const,
};

export const READINESS_REFRESH_MILLISECONDS = 10_000;

/** Creates an in-memory query client with conservative automatic retry behavior. */
export function createConsoleQueryClient() {
  return new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: {
        refetchOnWindowFocus: false,
        retry: false,
        staleTime: 0,
      },
    },
  });
}
