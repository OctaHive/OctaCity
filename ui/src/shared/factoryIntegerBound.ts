import { FACTORY_AUTHORING_SCHEMAS } from '../../.generated/api/factory-authoring';

const decimal = FACTORY_AUTHORING_SCHEMAS.FactoryIntegerBound.oneOf.find(
  (variant) => variant.type === 'string',
)!;
const pattern = new RegExp(decimal.pattern);

/** Exact schema metadata only; actual node values keep their ordinary JSON types. */
export function exactSchemaInteger(value: number | string): bigint | null {
  if (typeof value === 'number' ? !Number.isSafeInteger(value) : !pattern.test(value)) return null;
  const integer = BigInt(value);
  return BigInt.asIntN(64, integer) === integer ? integer : null;
}
