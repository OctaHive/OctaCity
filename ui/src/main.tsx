import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { createConsoleQueryClient } from './app/query';
import { createConsoleBrowserRouter } from './app/router';
import './styles/global.css';

const rootElement = document.getElementById('root');

if (rootElement === null) {
  throw new Error('Operator console root element is missing');
}

createRoot(rootElement).render(
  <StrictMode>
    <App queryClient={createConsoleQueryClient()} router={createConsoleBrowserRouter()} />
  </StrictMode>,
);
