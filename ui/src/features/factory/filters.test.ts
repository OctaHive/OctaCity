import { expect, it } from 'vitest';
import { readFactoryFilters, readFactoryProjectId, factorySearch } from './filters';

it('round-trips bounded shareable filters and excludes unknown or malformed values', () => {
  const filters = readFactoryFilters(
    new URLSearchParams(
      'state=waiting&decision=rework&delivery_state=unknown&source=manual&admitted_from_unix_ms=10&admitted_before_unix_ms=20&task=private',
    ),
  );
  expect(filters).toEqual({
    state: 'waiting',
    decision: 'rework',
    delivery_state: 'unknown',
    source: 'manual',
    admitted_from_unix_ms: 10,
    admitted_before_unix_ms: 20,
  });
  expect(readFactoryFilters(new URLSearchParams(factorySearch(filters)))).toEqual(filters);
  expect(
    readFactoryFilters(
      new URLSearchParams(
        'state=made-up&configuration_id=private&source=' +
          'x'.repeat(129) +
          '&admitted_from_unix_ms=-1&admitted_before_unix_ms=NaN',
      ),
    ),
  ).toEqual({});
  expect(
    readFactoryFilters(
      new URLSearchParams(
        'state=waiting&state=completed&admitted_from_unix_ms=20&admitted_before_unix_ms=10',
      ),
    ),
  ).toEqual({});
});

it('normalizes bounded Project identities without restricting the UUID version or accepting ambiguous ownership', () => {
  expect(
    readFactoryProjectId(new URLSearchParams('project_id=AAAAAAAA-AAAA-7AAA-8AAA-AAAAAAAAAAAA')),
  ).toBe('aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa');
  expect(readFactoryProjectId(new URLSearchParams('project_id=bad'))).toBeNull();
  expect(
    readFactoryProjectId(
      new URLSearchParams(
        'project_id=aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa&project_id=bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
      ),
    ),
  ).toBeNull();
});
