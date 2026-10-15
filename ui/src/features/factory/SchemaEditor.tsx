import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { useDraftRowKeys } from './useDraftRowKeys';
import { ScalarEditor } from './ScalarEditor';
import { useState } from 'react';
import type { FactoryApi } from './api';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';

type Shape = components['schemas']['FactoryFlowValueSchema'];
const MAX_DEPTH = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueSchema['x-max-depth'];
const MAX_ENUM_VALUES = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueSchema.oneOf.flatMap((variant) =>
  'values' in variant.properties ? [variant.properties.values.maxItems] : [],
)[0]!;
const VARIANT_CONTRACT = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueSchema.oneOf.flatMap(
  (variant) => ('variants' in variant.properties ? [variant.properties.variants] : []),
)[0]!;

const TYPES = {
  boolean: 'factory.boolean',
  integer: 'factory.integer',
  string: 'factory.string',
  object: 'factory.object',
  array: 'factory.array',
  enum: 'factory.enum',
  externally_tagged: 'factory.tagged',
} satisfies Record<Shape['type'], MessageKey>;
export function SchemaEditor({
  api,
  projectId,
  onAdd,
}: {
  api: Pick<FactoryApi, 'normalizeSchema'>;
  projectId: string;
  onAdd: (schema: components['schemas']['FactoryFlowDataSchema']) => void;
}) {
  const { t } = usePresentation();
  const [identity, setIdentity] = useState('');
  const [version, setVersion] = useState('');
  const [shape, setShape] = useState<Shape>({ type: 'boolean' });
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const add = async () => {
    setPending(true);
    setError(null);
    try {
      onAdd(await api.normalizeSchema({ project_id: projectId, identity, version, shape }));
      setIdentity('');
      setVersion('');
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error('Schema normalization failed'));
    } finally {
      setPending(false);
    }
  };
  return (
    <fieldset>
      <legend>{t('factory.schemas')}</legend>
      <label>
        {t('factory.schemaName')}
        <input
          maxLength={128}
          value={identity}
          onChange={(event) => setIdentity(event.target.value)}
        />
      </label>
      <label>
        {t('factory.schemaVersion')}
        <input
          maxLength={128}
          value={version}
          onChange={(event) => setVersion(event.target.value)}
        />
      </label>
      <ShapeEditor value={shape} onChange={setShape} />
      {error ? (
        <QueryFailureNotice title={t('factory.schemas')} error={error} onRetry={add} />
      ) : null}
      <button type="button" disabled={pending || !identity || !version} onClick={() => void add()}>
        {t('factory.addSchema')}
      </button>
    </fieldset>
  );
}
/** Recursive closed schema controls; depth and field counts are bounded before rendering. */
function ShapeEditor({
  value,
  onChange,
  depth = 0,
}: {
  value: Shape;
  onChange: (value: Shape) => void;
  depth?: number;
}) {
  const { t } = usePresentation();
  const fieldKeys = useDraftRowKeys(value.type === 'object' ? Object.values(value.fields) : []);
  const enumKeys = useDraftRowKeys(value.type === 'enum' ? value.values : []);
  const variantKeys = useDraftRowKeys(
    value.type === 'externally_tagged' ? Object.values(value.variants) : [],
  );
  return (
    <fieldset>
      <SelectMenu
        ariaLabel={t('factory.schemaType')}
        value={value.type}
        options={Object.entries(TYPES)
          .filter(([type]) => depth < MAX_DEPTH || (type !== 'object' && type !== 'array'))
          .map(([type, label]) => ({ value: type, label: t(label) }))}
        onValueChange={(type) => {
          switch (type) {
            case 'boolean':
              onChange({ type });
              break;
            case 'integer':
              onChange({ type, minimum: 0, maximum: 1 });
              break;
            case 'string':
              onChange({ type, min_bytes: 0, max_bytes: 128 });
              break;
            case 'object':
              onChange({ type, fields: {} });
              break;
            case 'array':
              onChange({ type, min_items: 0, max_items: 1, item: { type: 'boolean' } });
              break;
            case 'enum':
              onChange({ type, values: [] });
              break;
            case 'externally_tagged':
              onChange({ type, variants: {} });
          }
        }}
      />
      {value.type === 'integer' || value.type === 'string' || value.type === 'array' ? (
        <>
          <label>
            {t('factory.minimum')}
            <input
              type="number"
              min={value.type === 'integer' ? -Number.MAX_SAFE_INTEGER : 0}
              max={Number.MAX_SAFE_INTEGER}
              value={
                value.type === 'integer'
                  ? value.minimum
                  : value.type === 'string'
                    ? value.min_bytes
                    : value.min_items
              }
              onChange={(event) =>
                onChange(
                  value.type === 'integer'
                    ? { ...value, minimum: Number(event.target.value) }
                    : value.type === 'string'
                      ? { ...value, min_bytes: Number(event.target.value) }
                      : { ...value, min_items: Number(event.target.value) },
                )
              }
            />
          </label>
          <label>
            {t('factory.maximum')}
            <input
              type="number"
              max={Number.MAX_SAFE_INTEGER}
              value={
                value.type === 'integer'
                  ? value.maximum
                  : value.type === 'string'
                    ? value.max_bytes
                    : value.max_items
              }
              onChange={(event) =>
                onChange(
                  value.type === 'integer'
                    ? { ...value, maximum: Number(event.target.value) }
                    : value.type === 'string'
                      ? { ...value, max_bytes: Number(event.target.value) }
                      : { ...value, max_items: Number(event.target.value) },
                )
              }
            />
          </label>
        </>
      ) : null}
      {value.type === 'array' && depth < MAX_DEPTH ? (
        <ShapeEditor
          value={value.item}
          depth={depth + 1}
          onChange={(item) => onChange({ ...value, item })}
        />
      ) : null}
      {value.type === 'object' && depth < MAX_DEPTH ? (
        <>
          <button
            type="button"
            disabled={Object.keys(value.fields).length >= 64 || Object.hasOwn(value.fields, '')}
            onClick={() =>
              onChange({
                ...value,
                fields: { ...value.fields, '': { required: true, value: { type: 'boolean' } } },
              })
            }
          >
            {t('factory.addField')}
          </button>
          {Object.entries(value.fields).map(([name, field], index) => (
            <fieldset key={fieldKeys[index]}>
              <label>
                {t('factory.fieldName')}
                <input
                  maxLength={128}
                  value={name}
                  onChange={(event) => {
                    if (
                      event.target.value !== name &&
                      Object.hasOwn(value.fields, event.target.value)
                    )
                      return;
                    const fields = Object.fromEntries(
                      Object.entries(value.fields).map(([key, item]) => [
                        key === name ? event.target.value : key,
                        item,
                      ]),
                    );
                    onChange({ ...value, fields });
                  }}
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={field.required}
                  onChange={(event) =>
                    onChange({
                      ...value,
                      fields: {
                        ...value.fields,
                        [name]: { ...field, required: event.target.checked },
                      },
                    })
                  }
                />
                {t('factory.required')}
              </label>
              <ShapeEditor
                value={field.value}
                depth={depth + 1}
                onChange={(shape) =>
                  onChange({
                    ...value,
                    fields: { ...value.fields, [name]: { ...field, value: shape } },
                  })
                }
              />
              <button
                type="button"
                onClick={() =>
                  onChange({
                    ...value,
                    fields: Object.fromEntries(
                      Object.entries(value.fields).filter(([key]) => key !== name),
                    ),
                  })
                }
              >
                {t('factory.removeField')}
              </button>
            </fieldset>
          ))}
        </>
      ) : null}
      {value.type === 'enum' ? (
        <>
          <button
            type="button"
            disabled={value.values.length >= MAX_ENUM_VALUES}
            onClick={() => onChange({ ...value, values: [...value.values, ''] })}
          >
            {t('factory.addEnumValue')}
          </button>
          {value.values.map((item, index) => (
            <fieldset key={enumKeys[index]}>
              <ScalarEditor
                value={item}
                typeLabel={t('factory.enumValueType')}
                valueLabel={t('factory.enumValue')}
                onChange={(item) =>
                  onChange({
                    ...value,
                    values: value.values.map((old, position) => (position === index ? item : old)),
                  })
                }
              />
              <button
                type="button"
                onClick={() =>
                  onChange({
                    ...value,
                    values: value.values.filter((_, position) => position !== index),
                  })
                }
              >
                {t('factory.removeEnumValue')}
              </button>
            </fieldset>
          ))}
        </>
      ) : null}
      {value.type === 'externally_tagged' ? (
        <>
          <button
            type="button"
            disabled={
              Object.keys(value.variants).length >= VARIANT_CONTRACT.maxProperties ||
              Object.hasOwn(value.variants, '')
            }
            onClick={() => onChange({ ...value, variants: { ...value.variants, '': null } })}
          >
            {t('factory.addVariant')}
          </button>
          {Object.entries(value.variants).map(([name, shape], index) => (
            <fieldset key={variantKeys[index]}>
              <label>
                {t('factory.variantName')}
                <input
                  maxLength={VARIANT_CONTRACT.propertyNames.maxLength}
                  value={name}
                  onChange={(event) => {
                    if (
                      event.target.value !== name &&
                      Object.hasOwn(value.variants, event.target.value)
                    )
                      return;
                    onChange({
                      ...value,
                      variants: Object.fromEntries(
                        Object.entries(value.variants).map(([key, item]) => [
                          key === name ? event.target.value : key,
                          item,
                        ]),
                      ),
                    });
                  }}
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={shape !== null}
                  disabled={depth >= MAX_DEPTH}
                  onChange={(event) =>
                    onChange({
                      ...value,
                      variants: {
                        ...value.variants,
                        [name]: event.target.checked ? { type: 'boolean' } : null,
                      },
                    })
                  }
                />
                {t('factory.variantPayload')}
              </label>
              {shape !== null && depth < MAX_DEPTH ? (
                <ShapeEditor
                  value={shape}
                  depth={depth + 1}
                  onChange={(shape) =>
                    onChange({ ...value, variants: { ...value.variants, [name]: shape } })
                  }
                />
              ) : null}
              <button
                type="button"
                onClick={() =>
                  onChange({
                    ...value,
                    variants: Object.fromEntries(
                      Object.entries(value.variants).filter(([key]) => key !== name),
                    ),
                  })
                }
              >
                {t('factory.removeVariant')}
              </button>
            </fieldset>
          ))}
        </>
      ) : null}
    </fieldset>
  );
}
