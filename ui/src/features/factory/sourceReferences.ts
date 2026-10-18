import type { components } from '../../../.generated/api/schema';
type Binding = components['schemas']['WorkSourceBinding'];
/** Exact immutable pins keep a refreshed choice list from changing a draft's selection. */
export const sourceReferenceKey = (reference: Binding['resource']) =>
  JSON.stringify([reference.identity, reference.version, reference.digest]);
export const sourceBindingKey = (binding: Binding) =>
  JSON.stringify(
    [binding.installation, binding.capability, binding.configuration, binding.resource].map(
      sourceReferenceKey,
    ),
  );
