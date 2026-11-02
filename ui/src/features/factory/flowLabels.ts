import type { components } from '../../../.generated/api/schema';

/** Display metadata follows the exact published root; historical graphs fall back to their entry. */
export function flowLabel(
  flow: components['schemas']['FactoryFlowConfiguration'],
  fallback: string,
) {
  const root = flow.closure.definitions.find(
    (definition) =>
      definition.reference.id === flow.closure.root.id &&
      definition.reference.version === flow.closure.root.version,
  );
  return `${root?.name ?? root?.entry ?? fallback} · v${flow.closure.root.version}`;
}
