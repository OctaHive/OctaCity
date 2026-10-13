import { ScalarEditor } from './ScalarEditor';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { useDraftRowKeys } from './useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Mapping = components['schemas']['FactoryFlowValueMapping'];
const MAX_DEPTH = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueMapping['x-max-depth'];
const MAX_ITEMS = FACTORY_AUTHORING_SCHEMAS.FactoryFlowValueMapping.oneOf.flatMap((variant) =>
  'items' in variant.properties ? [variant.properties.items.maxItems] : [],
)[0]!;

const KINDS = {
  pointer: 'factory.pointer',
  constant: 'factory.constant',
  object: 'factory.object',
  array: 'factory.array',
  if_present: 'factory.ifPresent',
} satisfies Record<Mapping['mapping'], MessageKey>;
/** Typed mapping expressions; no script or arbitrary JSON editor is exposed. */
export function ValueMappingEditor({
  value,
  onChange,
  depth = 0,
}: {
  value: Mapping;
  onChange: (value: Mapping) => void;
  depth?: number;
}) {
  const { t } = usePresentation();
  const fieldKeys = useDraftRowKeys(value.mapping === 'object' ? Object.values(value.fields) : []);
  const itemKeys = useDraftRowKeys(value.mapping === 'array' ? value.items : []);
  return (
    <fieldset>
      <SelectMenu
        ariaLabel={t('factory.mappingKind')}
        value={value.mapping}
        options={Object.entries(KINDS)
          .filter(([kind]) => depth < MAX_DEPTH || kind === 'pointer' || kind === 'constant')
          .map(([kind, label]) => ({ value: kind, label: t(label) }))}
        onValueChange={(kind) => {
          switch (kind) {
            case 'pointer':
              onChange({ mapping: kind, path: '' });
              break;
            case 'constant':
              onChange({ mapping: kind, value: false });
              break;
            case 'object':
              onChange({ mapping: kind, fields: {} });
              break;
            case 'array':
              onChange({ mapping: kind, items: [] });
              break;
            case 'if_present':
              onChange({
                mapping: kind,
                path: '',
                then: { mapping: 'pointer', path: '' },
                otherwise: { mapping: 'constant', value: null },
              });
          }
        }}
      />
      {value.mapping === 'pointer' || value.mapping === 'if_present' ? (
        <label>
          {t('factory.selectionPath')}
          <input
            maxLength={1024}
            value={value.path}
            onChange={(event) => onChange({ ...value, path: event.target.value })}
          />
        </label>
      ) : null}
      {value.mapping === 'constant' ? (
        <ScalarEditor
          value={value.value}
          typeLabel={t('factory.constantType')}
          valueLabel={t('factory.constantValue')}
          onChange={(constant) => onChange({ ...value, value: constant })}
        />
      ) : null}
      {value.mapping === 'object' && depth < MAX_DEPTH ? (
        <>
          <button
            type="button"
            disabled={Object.keys(value.fields).length >= 64 || Object.hasOwn(value.fields, '')}
            onClick={() =>
              onChange({
                ...value,
                fields: { ...value.fields, '': { mapping: 'pointer', path: '' } },
              })
            }
          >
            {t('factory.addMappedField')}
          </button>
          {Object.entries(value.fields).map(([name, item], index) => (
            <fieldset key={fieldKeys[index]}>
              <label>
                {t('factory.mappedFieldName')}
                <input
                  maxLength={128}
                  value={name}
                  onChange={(event) => {
                    if (
                      event.target.value !== name &&
                      Object.hasOwn(value.fields, event.target.value)
                    )
                      return;
                    onChange({
                      ...value,
                      fields: Object.fromEntries(
                        Object.entries(value.fields).map(([key, item]) => [
                          key === name ? event.target.value : key,
                          item,
                        ]),
                      ),
                    });
                  }}
                />
              </label>
              <ValueMappingEditor
                value={item}
                depth={depth + 1}
                onChange={(item) =>
                  onChange({ ...value, fields: { ...value.fields, [name]: item } })
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
      {value.mapping === 'array' && depth < MAX_DEPTH ? (
        <>
          <button
            type="button"
            disabled={value.items.length >= MAX_ITEMS}
            onClick={() =>
              onChange({ ...value, items: [...value.items, { mapping: 'pointer', path: '' }] })
            }
          >
            {t('factory.addItem')}
          </button>
          {value.items.map((item, index) => (
            <fieldset key={itemKeys[index]}>
              <ValueMappingEditor
                value={item}
                depth={depth + 1}
                onChange={(item) =>
                  onChange({
                    ...value,
                    items: value.items.map((current, position) =>
                      position === index ? item : current,
                    ),
                  })
                }
              />
              <button
                type="button"
                onClick={() =>
                  onChange({
                    ...value,
                    items: value.items.filter((_, position) => position !== index),
                  })
                }
              >
                {t('factory.removeItem')}
              </button>
            </fieldset>
          ))}
        </>
      ) : null}
      {value.mapping === 'if_present' && depth < MAX_DEPTH ? (
        <>
          <fieldset>
            <legend>{t('factory.whenPresent')}</legend>
            <ValueMappingEditor
              value={value.then}
              depth={depth + 1}
              onChange={(then) => onChange({ ...value, then })}
            />
          </fieldset>
          <fieldset>
            <legend>{t('factory.otherwise')}</legend>
            <ValueMappingEditor
              value={value.otherwise}
              depth={depth + 1}
              onChange={(otherwise) => onChange({ ...value, otherwise })}
            />
          </fieldset>
        </>
      ) : null}
    </fieldset>
  );
}
