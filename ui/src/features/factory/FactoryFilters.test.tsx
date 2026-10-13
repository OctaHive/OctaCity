// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, it, expect } from 'vitest';
import { FactoryFiltersPanel } from './FactoryFilters';
afterEach(cleanup);
it('applies bounded source and admission-time filters to a shareable URL without retaining a continuation', async () => {
  const router = createMemoryRouter([{ path: '*', element: <FactoryFiltersPanel /> }], {
    initialEntries: ['/factory?state=waiting&after=old-page'],
  });
  render(<RouterProvider router={router} />);
  await userEvent.type(screen.getByRole('textbox', { name: 'Work source' }), 'operator.source');
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Admitted from (Unix ms)' }), '10');
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Admitted before (Unix ms)' }), '20');
  await userEvent.click(screen.getByRole('button', { name: 'Apply filters' }));
  expect(router.state.location.search).toBe(
    '?state=waiting&source=operator.source&admitted_from_unix_ms=10&admitted_before_unix_ms=20',
  );
  expect(screen.getByRole('textbox', { name: 'Work source' }).getAttribute('maxlength')).toBe(
    '128',
  );
});

it('visually names the lifecycle, decision and delivery controls as well as exposing accessible labels', () => {
  const router = createMemoryRouter([{ path: '*', element: <FactoryFiltersPanel /> }]);
  render(<RouterProvider router={router} />);
  expect(screen.getByText('Lifecycle state')).toBeTruthy();
  expect(screen.getByText('Decision')).toBeTruthy();
  expect(screen.getByText('Delivery')).toBeTruthy();
});
