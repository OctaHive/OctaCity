// @vitest-environment jsdom

import { describe, expect, it } from 'vitest';

import { auditFiltersFromForm } from './auditFilters';

describe('audit filter form', () => {
  it('rejects malformed non-empty dates instead of silently broadening the query', () => {
    const data = new FormData();
    data.set('occurred_from', 'not-a-date');

    expect(auditFiltersFromForm(data)).toEqual(
      expect.objectContaining({ error: 'Occurred from must be a valid local date and time.' }),
    );
  });
});
