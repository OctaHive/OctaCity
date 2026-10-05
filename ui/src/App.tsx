import { QueryClientProvider, type QueryClient } from '@tanstack/react-query';
import { RouterProvider, type RouterProviderProps } from 'react-router-dom';

import { PresentationProvider } from './app/presentation/PresentationProvider';
import { NotificationProvider } from './app/notifications/NotificationProvider';

interface AppProps {
  queryClient: QueryClient;
  router: RouterProviderProps['router'];
}

/** Installs the console's presentation, server-state, notification, and routing providers. */
export function App({ queryClient, router }: AppProps) {
  return (
    <PresentationProvider>
      <QueryClientProvider client={queryClient}>
        <NotificationProvider>
          <RouterProvider router={router} />
        </NotificationProvider>
      </QueryClientProvider>
    </PresentationProvider>
  );
}
