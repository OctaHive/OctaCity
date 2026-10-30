import type { components } from '../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../.generated/api/factory-authoring';
import { usePresentation } from '../app/presentation/PresentationProvider';
import { SelectMenu } from './SelectMenu';
import { useDraftRowKeys } from './useDraftRowKeys';
import { exactSchemaInteger } from './factoryIntegerBound';

export type FactoryValueSchema = components['schemas']['FactoryFlowValueSchema'];
const MAX_DEPTH = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueSchema['x-max-depth'];
export function initialFactoryValue(schema: FactoryValueSchema, depth = 0): unknown {
  if (depth > MAX_DEPTH) return undefined;
  switch (schema.type) {
    case 'boolean':
      return false;
    case 'integer': {
      const minimum = exactSchemaInteger(schema.minimum);
      const maximum = exactSchemaInteger(schema.maximum);
      if (minimum === null || maximum === null) return undefined;
      const first =
        minimum < BigInt(Number.MIN_SAFE_INTEGER) ? BigInt(Number.MIN_SAFE_INTEGER) : minimum;
      return first <= maximum && first <= BigInt(Number.MAX_SAFE_INTEGER)
        ? Number(first)
        : undefined;
    }
    case 'string':
      return '';
    case 'enum':
      return schema.values[0];
    case 'externally_tagged': {
      const first = Object.entries(schema.variants)[0];
      if (!first) return undefined;
      const [name, payload] = first;
      return payload === null ? name : { [name]: initialFactoryValue(payload, depth + 1) };
    }
    case 'array':
      return [];
    case 'object':
      return Object.fromEntries(
        Object.entries(schema.fields)
          .filter(([, field]) => field.required)
          .map(([key, field]) => [key, initialFactoryValue(field.value, depth + 1)]),
      );
  }
}
export function validFactoryValue(schema: FactoryValueSchema, value: unknown, depth = 0): boolean {
  if (depth > MAX_DEPTH) return false;
  switch (schema.type) {
    case 'boolean':
      return typeof value === 'boolean';
    case 'integer': {
      const minimum = exactSchemaInteger(schema.minimum);
      const maximum = exactSchemaInteger(schema.maximum);
      return (
        typeof value === 'number' &&
        Number.isSafeInteger(value) &&
        minimum !== null &&
        maximum !== null &&
        BigInt(value) >= minimum &&
        BigInt(value) <= maximum
      );
    }
    case 'string': {
      const bytes = typeof value === 'string' ? new TextEncoder().encode(value).length : -1;
      return bytes >= schema.min_bytes && bytes <= schema.max_bytes;
    }
    case 'enum':
      return schema.values.some((item) => Object.is(value, item));
    case 'externally_tagged': {
      if (typeof value === 'string')
        return Object.hasOwn(schema.variants, value) && schema.variants[value] === null;
      if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
      const entries = Object.entries(value);
      if (entries.length !== 1) return false;
      const [name, payload] = entries[0]!;
      if (!Object.hasOwn(schema.variants, name)) return false;
      const shape = schema.variants[name];
      return shape !== null && shape !== undefined && validFactoryValue(shape, payload, depth + 1);
    }
    case 'array':
      return (
        Array.isArray(value) &&
        value.length >= schema.min_items &&
        value.length <= schema.max_items &&
        value.every((item) => validFactoryValue(schema.item, item, depth + 1))
      );
    case 'object': {
      if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
      const record = value as Record<string, unknown>;
      return (
        Object.keys(record).every((key) => Object.hasOwn(schema.fields, key)) &&
        Object.entries(schema.fields).every(([key, field]) =>
          Object.hasOwn(record, key)
            ? validFactoryValue(field.value, record[key], depth + 1)
            : !field.required,
        )
      );
    }
  }
}
/** Controlled observations follow the pinned schema; this form owns no lifecycle authority. */
export function FactoryValueInput({
  schema,
  value,
  onChange,
  label,
  depth = 0,
}: {
  schema: FactoryValueSchema;
  value: unknown;
  onChange: (value: unknown) => void;
  label: string;
  depth?: number;
}) {
  const { t } = usePresentation();
  if (depth > MAX_DEPTH) return <p>{t('factory.invalidContract')}</p>;
  if (schema.type === 'boolean')
    return (
      <label>
        <input
          type="checkbox"
          checked={value === true}
          onChange={(event) => onChange(event.currentTarget.checked)}
        />
        {label}
      </label>
    );
  if (schema.type === 'integer')
    return (
      <label>
        {label}
        <input
          type="number"
          min={schema.minimum}
          max={schema.maximum}
          value={typeof value === 'number' ? value : ''}
          aria-invalid={!validFactoryValue(schema, value)}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
      </label>
    );
  if (schema.type === 'string')
    return (
      <label>
        {label}
        <input
          maxLength={schema.max_bytes}
          value={typeof value === 'string' ? value : ''}
          autoComplete="off"
          aria-invalid={!validFactoryValue(schema, value)}
          onChange={(event) => onChange(event.currentTarget.value)}
        />
      </label>
    );
  if (schema.type === 'enum')
    return (
      <SelectMenu
        ariaLabel={label}
        value={String(schema.values.findIndex((item) => Object.is(item, value)))}
        onValueChange={(index) => {
          const position = Number(index);
          if (Number.isSafeInteger(position) && position >= 0 && position < schema.values.length)
            onChange(schema.values[position]);
        }}
        options={schema.values.map((item, index) => ({
          value: String(index),
          label: item === null ? 'null' : `${typeof item}: ${String(item)}`,
        }))}
      />
    );
  if (schema.type === 'array')
    return (
      <ArrayValue schema={schema} value={value} onChange={onChange} label={label} depth={depth} />
    );
  if (schema.type === 'externally_tagged') {
    const entries =
      value !== null && typeof value === 'object' && !Array.isArray(value)
        ? Object.entries(value)
        : [];
    const name = typeof value === 'string' ? value : entries.length === 1 ? entries[0]![0] : '';
    const shape = Object.hasOwn(schema.variants, name) ? schema.variants[name] : undefined;
    return (
      <fieldset>
        <legend>{label}</legend>
        <SelectMenu
          ariaLabel={t('factory.variantSelection', { label })}
          value={name}
          options={Object.keys(schema.variants).map((name) => ({ value: name, label: name }))}
          onValueChange={(name) => {
            if (!Object.hasOwn(schema.variants, name)) return;
            const shape = schema.variants[name];
            if (shape !== undefined)
              onChange(shape === null ? name : { [name]: initialFactoryValue(shape, depth + 1) });
          }}
        />
        {shape !== null && shape !== undefined ? (
          <FactoryValueInput
            schema={shape}
            value={entries[0]?.[1]}
            onChange={(payload) => onChange({ [name]: payload })}
            label={`${label} · ${name}`}
            depth={depth + 1}
          />
        ) : null}
      </fieldset>
    );
  }
  if (schema.type === 'object') {
    const record =
      value !== null && typeof value === 'object' && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {};
    return (
      <fieldset>
        <legend>{label}</legend>
        {Object.entries(schema.fields).map(([key, field]) => (
          <section key={key}>
            {!field.required ? (
              <label>
                <input
                  type="checkbox"
                  checked={Object.hasOwn(record, key)}
                  onChange={(event) => {
                    if (event.currentTarget.checked)
                      onChange({ ...record, [key]: initialFactoryValue(field.value, depth + 1) });
                    else {
                      const next = { ...record };
                      delete next[key];
                      onChange(next);
                    }
                  }}
                />
                {t('factory.includeObservation', { label: `${label} · ${key}` })}
              </label>
            ) : null}
            {field.required || Object.hasOwn(record, key) ? (
              <FactoryValueInput
                schema={field.value}
                value={record[key]}
                onChange={(next) => onChange({ ...record, [key]: next })}
                label={`${label} · ${key}`}
                depth={depth + 1}
              />
            ) : null}
          </section>
        ))}
      </fieldset>
    );
  }
  return null;
}

function ArrayValue({
  schema,
  value,
  onChange,
  label,
  depth,
}: {
  schema: Extract<FactoryValueSchema, { type: 'array' }>;
  value: unknown;
  onChange: (value: unknown) => void;
  label: string;
  depth: number;
}) {
  const { t } = usePresentation();
  const items: unknown[] = Array.isArray(value) ? value : [];
  const keys = useDraftRowKeys(items);
  return (
    <fieldset>
      <legend>{label}</legend>
      {items.map((item, index) => (
        <section key={keys[index]}>
          <FactoryValueInput
            schema={schema.item}
            value={item}
            onChange={(next) =>
              onChange(items.map((value, position) => (position === index ? next : value)))
            }
            label={`${label} · ${index + 1}`}
            depth={depth + 1}
          />
          <button
            type="button"
            onClick={() => onChange(items.filter((_, position) => position !== index))}
          >
            {t('factory.removeObservation')}
          </button>
        </section>
      ))}
      <button
        type="button"
        disabled={items.length >= schema.max_items}
        onClick={() => onChange([...items, initialFactoryValue(schema.item, depth + 1)])}
      >
        {t('factory.addObservation')}
      </button>
    </fieldset>
  );
}
