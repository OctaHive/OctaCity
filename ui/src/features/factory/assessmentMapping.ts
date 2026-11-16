import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
type S = components['schemas'];

/** Preserve optional fields, including declared assessment dependencies, without inventing values. */
export function assessmentMapping(
  fields: Extract<S['FactoryFlowValueSchema'], { type: 'object' }>['fields'],
  alias = 'assessment',
): S['FactoryFlowValueMapping'] {
  const selected = Object.entries(fields);
  const maximum = FACTORY_AUTHORING_SCHEMAS.FactoryFlowDataMapping['x-max-encoded-bytes'];
  const encoded = (value: unknown) => new TextEncoder().encode(JSON.stringify(value)).length;
  const tooLarge = () => new RangeError('Issue projection exceeds the input mapping limits');
  if (
    selected.filter(([, field]) => !field.required).length + 1 >
    FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueMapping['x-max-depth']
  )
    throw tooLarge();
  let bytes = encoded({ value: null }) - encoded(null);
  const reserve = (amount: number) => {
    bytes += amount;
    if (bytes > maximum) throw tooLarge();
  };
  const build = (
    index: number,
    projection: Record<string, S['FactoryFlowValueMapping']>,
  ): S['FactoryFlowValueMapping'] => {
    const field = selected[index];
    if (!field) {
      const value = { mapping: 'object' as const, fields: projection };
      reserve(encoded(value));
      return value;
    }
    const [name, schema] = field;
    const path = `/${alias}/${name}`;
    const present = build(index + 1, { ...projection, [name]: { mapping: 'pointer', path } });
    if (schema.required) return present;
    reserve(
      encoded({ mapping: 'if_present', path, then: null, otherwise: null }) - 2 * encoded(null),
    );
    return { mapping: 'if_present', path, then: present, otherwise: build(index + 1, projection) };
  };
  return build(0, {});
}
