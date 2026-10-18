import { readFileSync } from 'node:fs';
import type { components } from '../../.generated/api/schema';

/** Canonical synthetic examples are validated against OpenAPI by the REST suite. */
export function integrationFixture<Name extends keyof components['schemas']>(
  name: Name,
  index = 0,
): components['schemas'][Name] {
  return JSON.parse(
    readFileSync(
      '../server/api/octacity-server-api-rest/fixtures/v1/integration-contracts.json',
      'utf8',
    ),
  )[name][index];
}
