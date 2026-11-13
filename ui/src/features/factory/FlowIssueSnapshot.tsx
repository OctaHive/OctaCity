import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import { issueFieldLabel, type IssueFieldChoice } from './IssueFieldContracts';
import { assessmentMapping } from './issueClassification';
import { editableSnapshotFields } from './issueSnapshotBindings';
import { equivalentFlowSchema, registerFlowSchemas } from './flowSchemas';

type S = components['schemas'];
type Draft = S['FactoryFlowDefinitionDraft'];
type Fields = Extract<S['FactoryFlowValueSchema'], { type: 'object' }>['fields'];
type Selection = { source: S['FactoryFlowIssueFieldSource']; required: boolean };
const limits = FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueMaterialSelection.properties;
const keys =
  FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueSnapshotInput.properties.fields.propertyNames;
const validKey = (key: string) =>
  new RegExp(keys.pattern).test(key) && key.length <= keys.maxLength;
const sourceKey = (source: IssueFieldChoice['source']) =>
  JSON.stringify(
    source.kind === 'property'
      ? [source.kind, source.id]
      : source.kind === 'classification'
        ? [source.kind, source.slot]
        : [source.kind],
  );
const defaultAlias = (field: IssueFieldChoice, index: number) => {
  const name =
    field.source.kind === 'property'
      ? field.source.id
      : field.source.kind === 'classification'
        ? field.source.slot
        : field.source.kind;
  return validKey(name) ? name : `field${index}`;
};

/** Snapshot authoring is an explicit input contract. It neither links Runs nor grants step access. */
export function FlowIssueSnapshot({
  value,
  services,
  subflows = [],
  onChange,
  onEditingChange,
}: {
  value: Draft;
  services: FlowAuthoringServices;
  subflows?: readonly S['FactoryFlowDefinitionDetails'][];
  onChange: (value: Draft) => void;
  onEditingChange: (editing: boolean) => void;
}) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const mixed = value.root.nodes.some(
    (node) =>
      editableSnapshotFields(node, value.issue_snapshot, value.data_schemas) === null &&
      Object.values(node.input_binding?.sources ?? {}).some(
        (source) =>
          source.source === 'incoming' &&
          source.schema.digest === value.issue_snapshot?.schema.digest,
      ),
  );
  return (
    <section aria-label={t('factory.issueSnapshot')}>
      <h3>{t('factory.issueSnapshot')}</h3>
      <p>{t('factory.issueSnapshotHelp')}</p>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
          onEditingChange(!open);
        }}
      >
        {t('factory.configureIssueSnapshot')}
      </button>
      {value.issue_snapshot ? (
        <button
          type="button"
          disabled={open || mixed}
          onClick={() => {
            const { issue_snapshot: removed, ...rest } = value;
            const nodes = value.root.nodes.map((node) =>
              editableSnapshotFields(node, removed, value.data_schemas) !== null
                ? { ...node, input_binding: null, input_schema: null }
                : node,
            );
            onChange({
              ...rest,
              root: {
                ...rest.root,
                nodes,
                input_schema:
                  nodes.find((node) => node.key === rest.root.entry)?.input_schema ?? null,
              },
            });
          }}
        >
          {t('factory.removeIssueSnapshot')}
        </button>
      ) : null}
      {open ? (
        <SnapshotDraft
          value={value}
          services={services}
          subflows={subflows}
          onChange={(next) => {
            onChange(next);
            setOpen(false);
            onEditingChange(false);
          }}
          onCancel={() => {
            setOpen(false);
            onEditingChange(false);
          }}
        />
      ) : null}
    </section>
  );
}

function SnapshotDraft({
  value,
  services,
  subflows,
  onChange,
  onCancel,
}: {
  value: Draft;
  services: FlowAuthoringServices;
  subflows: readonly S['FactoryFlowDefinitionDetails'][];
  onChange: (value: Draft) => void;
  onCancel: () => void;
}) {
  const { t } = usePresentation();
  const query = useFactoryAuthoringCapabilities(services, true, 0);
  const choices = query.data?.issue_fields;
  // Local selections belong to the pipeline revision on which this form was opened.
  const [baseline] = useState(value);
  const stale = value !== baseline;
  const [fields, setFields] = useState<Record<string, Selection>>(
    value.issue_snapshot?.fields ?? {},
  );
  const [materials, setMaterials] = useState(!!value.issue_snapshot?.materials?.materials);
  const existingMaterial = value.issue_snapshot?.materials?.materials;
  const [entries, setEntries] = useState(
    existingMaterial?.source.kind === 'selected' ? existingMaterial.source.max_entries : 1,
  );
  const [bytes, setBytes] = useState(existingMaterial?.max_bytes ?? limits.max_bytes.maximum);
  const [materialRequired, setMaterialRequired] = useState(existingMaterial?.required ?? false);
  const [projections, setProjections] = useState<Record<string, string[]>>(() =>
    Object.fromEntries(
      value.root.nodes.map((node) => [
        node.key,
        editableSnapshotFields(node, value.issue_snapshot, value.data_schemas) ?? [],
      ]),
    ),
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const controllerRef = useRef<AbortController | null>(null);
  useEffect(() => () => controllerRef.current?.abort(), []);
  const latestRef = useRef({ value, choices });
  latestRef.current = { value, choices };
  const selected = Object.entries(fields).flatMap(([alias, selection]) => {
    const field = choices?.fields.find(
      (field) => sourceKey(field.source) === sourceKey(selection.source),
    );
    return field ? [{ alias, selection, field }] : [];
  });
  const unavailable = Object.entries(fields).filter(
    ([alias]) => !selected.some((field) => field.alias === alias),
  );
  const aliases = [
    ...selected.map(({ alias, field }) => ({ alias, label: issueFieldLabel(field, t) })),
    ...(materials ? [{ alias: 'materials', label: t('factory.issueMaterials') }] : []),
  ];
  // Only replace this editor's exclusive Incoming bindings. Mixed/advanced inputs stay explicit.
  const conflict =
    !!value.accepted_predecessor ||
    value.root.nodes.some((node) => {
      const consumed = projections[node.key]?.length;
      if (!node.input_binding && !node.input_schema) return false;
      const owned = editableSnapshotFields(node, value.issue_snapshot, value.data_schemas) !== null;
      const previous = Object.values(node.input_binding?.sources ?? {}).some(
        (source) =>
          source.source === 'incoming' &&
          source.schema.digest === value.issue_snapshot?.schema.digest,
      );
      return !owned && (!!consumed || previous);
    }) ||
    !!value.issue_snapshot?.admission ||
    !!Object.keys(value.issue_snapshot?.evidence ?? {}).length ||
    Object.keys(value.issue_snapshot?.materials ?? {}).some((alias) => alias !== 'materials') ||
    (existingMaterial && existingMaterial.source.kind !== 'selected');
  const valid =
    choices &&
    !query.isFetching &&
    !query.error &&
    !stale &&
    !conflict &&
    selected.length === Object.keys(fields).length &&
    selected.every(({ selection, field }) => !selection.required || field.required_supported) &&
    aliases.length > 0 &&
    aliases.every(({ alias }) => validKey(alias)) &&
    new Set(aliases.map(({ alias }) => alias)).size === aliases.length &&
    value.root.nodes.some((node) =>
      projections[node.key]?.some((key) => aliases.some(({ alias }) => alias === key)),
    ) &&
    (!materials ||
      (Number.isSafeInteger(entries) &&
        entries >= 1 &&
        entries <=
          FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueMaterialSelection.properties.source.oneOf[1]
            .properties.max_entries.maximum &&
        Number.isSafeInteger(bytes) &&
        bytes >= 1 &&
        bytes <= limits.max_bytes.maximum));
  async function apply() {
    if (!valid || !choices || pending) return;
    const controller = new AbortController();
    controllerRef.current = controller;
    setPending(true);
    setError(null);
    try {
      const shapeFields: Fields = Object.fromEntries(
        selected.map(({ alias, selection, field }) => [
          alias,
          { required: selection.required, value: field.shape },
        ]),
      );
      if (materials)
        shapeFields.materials = {
          required: materialRequired,
          value: {
            type: 'array',
            min_items: materialRequired ? 1 : 0,
            max_items: entries,
            item: { type: 'string', min_bytes: 0, max_bytes: bytes },
          },
        };
      const plans = value.root.nodes.map((node) => {
        const names = projections[node.key]?.filter((key) => Object.hasOwn(shapeFields, key)) ?? [];
        const projection = Object.fromEntries(names.map((name) => [name, shapeFields[name]!]));
        return {
          node,
          names,
          projection,
          mapping:
            names.length === aliases.length
              ? { mapping: 'pointer' as const, path: '/issue' }
              : names.length
                ? assessmentMapping(projection, 'issue')
                : null,
        };
      });
      const schemas: S['FactoryFlowDataSchema'][] = [];
      const normalize = async (fields: Fields) => {
        const shape: S['FactoryFlowValueSchema'] = { type: 'object', fields };
        const schema =
          equivalentFlowSchema([...value.data_schemas, ...schemas], shape) ??
          (await services.api.normalizeSchema(
            {
              project_id: services.projectId,
              identity: `issue-snapshot.${crypto.randomUUID()}`,
              version: '1',
              shape,
            },
            controller.signal,
          ));
        schemas.push(schema);
        return schema;
      };
      const input = await normalize(shapeFields);
      const nodes: S['FactoryFlowNodeDefinition'][] = [];
      for (const { node, names, projection, mapping } of plans) {
        if (!names.length) {
          const old =
            editableSnapshotFields(node, value.issue_snapshot, value.data_schemas) !== null;
          nodes.push(old ? { ...node, input_binding: null, input_schema: null } : node);
          continue;
        }
        const schema = names.length === aliases.length ? input : await normalize(projection);
        const binding = {
          sources: { issue: { source: 'incoming' as const, schema: input.reference, path: '' } },
          mapping: { value: mapping! },
        };
        if (
          new TextEncoder().encode(JSON.stringify(binding)).length >
          FACTORY_AUTHORING_SCHEMAS.FactoryFlowInputBinding['x-max-encoded-bytes']
        )
          throw new RangeError();
        nodes.push({ ...node, input_schema: schema.reference, input_binding: binding });
      }
      if (controller.signal.aborted) return;
      if (latestRef.current.value !== value || latestRef.current.choices !== choices)
        throw new Error(t('factory.classificationDraftChanged'));
      const entry = nodes.find((node) => node.key === value.root.entry);
      const replaced = [
        ...(value.issue_snapshot ? [value.issue_snapshot.schema] : []),
        ...value.root.nodes.flatMap((node) =>
          editableSnapshotFields(node, value.issue_snapshot, value.data_schemas) !== null &&
          node.input_schema
            ? [node.input_schema]
            : [],
        ),
      ];
      onChange(
        registerFlowSchemas(
          {
            ...value,
            issue_snapshot: {
              version: 1,
              definition_version: choices.definition_version,
              schema: input.reference,
              fields,
              ...(materials
                ? {
                    materials: {
                      materials: {
                        source: { kind: 'selected', max_entries: entries },
                        required: materialRequired,
                        max_bytes: bytes,
                      },
                    },
                  }
                : {}),
            },
            root: {
              ...value.root,
              nodes,
              input_schema: entry?.input_schema ?? value.root.input_schema,
            },
          },
          schemas,
          replaced,
          subflows,
        ),
      );
    } catch (failure) {
      if (!controller.signal.aborted)
        setError(failure instanceof Error ? failure : new Error(String(failure)));
    } finally {
      if (!controller.signal.aborted) {
        controllerRef.current = null;
        setPending(false);
      }
    }
  }
  return (
    <fieldset disabled={pending}>
      {stale ? <p role="alert">{t('factory.snapshotDraftChanged')}</p> : null}
      {query.isFetching ? <p role="status">{t('factory.loadingStepCapabilities')}</p> : null}
      {query.error ? (
        <QueryFailureNotice
          title={t('factory.issueSnapshot')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : null}
      {choices ? (
        <>
          <p>{t('planning.definition', { version: choices.definition_version })}</p>
          {unavailable.map(([alias, selection]) => (
            <div key={alias}>
              <p role="alert">{t('factory.unavailableSnapshotField', { alias })}</p>
              <p>{issueFieldLabel({ source: selection.source }, t)}</p>
              <p>{t('factory.unavailableSnapshotFieldHelp')}</p>
              <button
                type="button"
                onClick={() => {
                  setFields(
                    Object.fromEntries(Object.entries(fields).filter(([name]) => name !== alias)),
                  );
                  setProjections(
                    Object.fromEntries(
                      Object.entries(projections).map(([key, names]) => [
                        key,
                        names.filter((name) => name !== alias),
                      ]),
                    ),
                  );
                }}
              >
                {t('factory.removeUnavailableSnapshotField', { alias })}
              </button>
            </div>
          ))}
          {choices.fields.map((field, index) => {
            const current = selected.find(
              ({ selection }) => sourceKey(selection.source) === sourceKey(field.source),
            );
            const label = issueFieldLabel(field, t);
            return (
              <div key={sourceKey(field.source)}>
                <label>
                  <input
                    type="checkbox"
                    checked={!!current}
                    onChange={(event) => {
                      if (event.target.checked) {
                        let alias = defaultAlias(field, index);
                        let suffix = index;
                        while (Object.hasOwn(fields, alias)) alias = `field${suffix++}`;
                        setFields({
                          ...fields,
                          [alias]: { source: field.source, required: false },
                        });
                      } else
                        setFields(
                          Object.fromEntries(
                            Object.entries(fields).filter(([alias]) => alias !== current?.alias),
                          ),
                        );
                    }}
                  />
                  {label}
                </label>
                {current ? (
                  <>
                    <label>
                      {t('factory.inputAlias', { name: label })}
                      <input
                        value={current.alias}
                        maxLength={keys.maxLength}
                        onChange={(event) => {
                          const alias = event.target.value;
                          if (Object.hasOwn(fields, alias) && alias !== current.alias) return;
                          setFields(
                            Object.fromEntries(
                              Object.entries(fields).map(([key, selection]) => [
                                key === current.alias ? alias : key,
                                selection,
                              ]),
                            ),
                          );
                          setProjections(
                            Object.fromEntries(
                              Object.entries(projections).map(([key, names]) => [
                                key,
                                names.map((name) => (name === current.alias ? alias : name)),
                              ]),
                            ),
                          );
                        }}
                      />
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        disabled={!field.required_supported && !current.selection.required}
                        checked={current.selection.required}
                        onChange={(event) =>
                          setFields({
                            ...fields,
                            [current.alias]: {
                              ...current.selection,
                              required: event.target.checked,
                            },
                          })
                        }
                      />
                      {t('factory.requiredIssueField', { name: label })}
                    </label>
                  </>
                ) : null}
              </div>
            );
          })}
          <label>
            <input
              type="checkbox"
              checked={materials}
              onChange={(event) => setMaterials(event.target.checked)}
            />
            {t('factory.includeIssueMaterials')}
          </label>
          {materials ? (
            <>
              <p>{t('factory.selectedMaterialsHelp')}</p>
              <label>
                {t('factory.maximumMaterialEntries')}
                <input
                  type="number"
                  min={1}
                  max={
                    FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssueMaterialSelection.properties.source
                      .oneOf[1].properties.max_entries.maximum
                  }
                  value={entries || ''}
                  onChange={(event) => setEntries(Number(event.target.value))}
                />
              </label>
              <label>
                {t('factory.maximumMaterialBytes')}
                <input
                  type="number"
                  min={1}
                  max={limits.max_bytes.maximum}
                  value={bytes || ''}
                  onChange={(event) => setBytes(Number(event.target.value))}
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={materialRequired}
                  onChange={(event) => setMaterialRequired(event.target.checked)}
                />
                {t('factory.requiredIssueField', { name: t('factory.issueMaterials') })}
              </label>
            </>
          ) : null}
          <fieldset>
            <legend>{t('factory.snapshotProjections')}</legend>
            {value.root.nodes.map((node) => (
              <fieldset key={node.key}>
                <legend>{node.key}</legend>
                {aliases.map(({ alias, label }) => (
                  <label key={alias}>
                    <input
                      type="checkbox"
                      checked={projections[node.key]?.includes(alias) ?? false}
                      onChange={(event) =>
                        setProjections({
                          ...projections,
                          [node.key]: event.target.checked
                            ? [...(projections[node.key] ?? []), alias]
                            : (projections[node.key]?.filter((key) => key !== alias) ?? []),
                        })
                      }
                    />
                    {node.key}: {label}
                  </label>
                ))}
              </fieldset>
            ))}
          </fieldset>
        </>
      ) : (
        <p>{t('factory.issueFieldsUnavailable')}</p>
      )}
      {conflict ? <p role="alert">{t('factory.snapshotInputConflict')}</p> : null}
      {error instanceof RangeError ? (
        <p role="alert">{t('factory.projectionTooLarge')}</p>
      ) : error ? (
        <QueryFailureNotice
          title={t('factory.issueSnapshot')}
          error={error}
          onRetry={() => void apply()}
        />
      ) : null}
      <button type="button" disabled={!valid || pending} onClick={() => void apply()}>
        {t('factory.useIssueSnapshot')}
      </button>
      <button type="button" onClick={onCancel}>
        {t('projectDocument.cancel')}
      </button>
    </fieldset>
  );
}
