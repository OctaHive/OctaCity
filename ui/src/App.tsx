import { QueryClientProvider, type QueryClient } from '@tanstack/react-query';
import { RouterProvider, type RouterProviderProps } from 'react-router-dom';

interface AppProps {
  queryClient: QueryClient;
  router: RouterProviderProps['router'];
}

/** Installs the console's only server-state and URL-routing providers. */
export function App({ queryClient, router }: AppProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
