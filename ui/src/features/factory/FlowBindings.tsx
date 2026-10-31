import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ValueMappingEditor } from './ValueMappingEditor';
import { SelectMenu } from '../../shared/SelectMenu';
import { FlowIssueStatus, type FlowIssueServices } from './FlowIssueStatus';
import { FlowSourceUpdate, type FlowSourceServices } from './FlowSourceUpdate';
import { documentSelections } from './flowDocumentBindings';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
type Root = components['schemas']['FactoryFlowRootDraft'];
type Schema = components['schemas']['FactoryFlowDataSchema'];
type Source = components['schemas']['FactoryFlowInputSource'];
const SOURCES = {
  node_configuration: 'factory.source.node_configuration',
  caller_input: 'factory.source.caller_input',
  predecessor: 'factory.source.predecessor',
  incoming: 'factory.source.incoming',
  work: 'factory.source.work',
  record: 'factory.source.record',
} satisfies Record<Source['source'], MessageKey>;
const CONTEXT = {
  task: 'factory.context.task',
  specification: 'factory.context.specification',
  typed_result: 'factory.context.typed_result',
  bounded_summary: 'factory.context.bounded_summary',
  evidence: 'factory.context.evidence',
  policy: 'factory.context.policy',
  repository_range: 'factory.context.repository_range',
  repository_fragment: 'factory.context.repository_fragment',
} satisfies Record<components['schemas']['FactoryFlowContextSelection']['source_kind'], MessageKey>;
/** Bounded input selectors and explicit Context entries, separate from control dependencies. */
export function FlowBindings({
  node,
  root,
  schemas,
  onChange,
  sources: services,
  issueStatus,
  documentsManaged = false,
}: {
  node: Node;
  root: Root;
  schemas: readonly Schema[];
  onChange: (node: Node) => void;
  sources?: FlowSourceServices | undefined;
  issueStatus?: FlowIssueServices | undefined;
  documentsManaged?: boolean;
}) {
  const { t } = usePresentation();
  const sources = node.input_binding?.sources ?? {};
  const entries = node.context_binding?.entries ?? {};
  const sourceKeys = useDraftRowKeys(Object.values(sources));
  const documentKeys = new Set(
    documentsManaged ? documentSelections(node).map((entry) => entry.key) : [],
  );
  const editableEntries = Object.entries(entries).filter(([key]) => !documentKeys.has(key));
  const entryKeys = useDraftRowKeys(editableEntries.map(([, entry]) => entry));
  const updateSources = (sources: Record<string, Source>) =>
    onChange({
      ...node,
      input_binding: {
        mapping: node.input_binding?.mapping ?? { value: { mapping: 'object', fields: {} } },
        sources,
      },
    });
  const updateEntries = (
    entries: Record<string, components['schemas']['FactoryFlowContextSelection']>,
  ) => onChange({ ...node, context_binding: { ...node.context_binding, entries } });
  return (
    <section>
      {issueStatus && node.action?.action === 'transition_issue' ? (
        <FlowIssueStatus key={node.key} node={node} services={issueStatus} onChange={onChange} />
      ) : null}
      {services && node.action?.action === 'set_status' ? (
        <FlowSourceUpdate key={node.key} node={node} services={services} onChange={onChange} />
      ) : null}
      <SelectMenu
        ariaLabel={t('factory.inputSchema')}
        value={String(
          schemas.findIndex((schema) => schema.reference.digest === node.input_schema?.digest),
        )}
        options={[
          { value: '-1', label: t('factory.none') },
          ...schemas.map((schema, index) => ({
            value: String(index),
            label: `${schema.reference.identity} · ${schema.reference.version}`,
          })),
        ]}
        onValueChange={(index) =>
          onChange({ ...node, input_schema: schemas[Number(index)]?.reference ?? null })
        }
      />
      <h4>{t('factory.inputMapping')}</h4>
      <ValueMappingEditor
        value={node.input_binding?.mapping.value ?? { mapping: 'object', fields: {} }}
        onChange={(value) => onChange({ ...node, input_binding: { sources, mapping: { value } } })}
      />
      <h4>{t('factory.inputSources')}</h4>
      <button
        type="button"
        disabled={Object.keys(sources).length >= 64 || Object.hasOwn(sources, '')}
        onClick={() => updateSources({ ...sources, '': { source: 'work', path: '' } })}
      >
        {t('factory.addInputSource')}
      </button>
      {Object.entries(sources).map(([name, source], index) => (
        <fieldset key={sourceKeys[index]}>
          <label>
            {t('factory.inputSourceName')}
            <input
              maxLength={128}
              value={name}
              onChange={(event) => updateSources(rename(sources, name, event.target.value))}
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.inputSource')}
            value={source.source}
            options={Object.entries(SOURCES)
              .filter(([kind]) => kind !== 'incoming' || schemas.length > 0)
              .map(([kind, label]) => ({ value: kind, label: t(label) }))}
            onValueChange={(kind) => {
              const schema = schemas[0];
              const first = root.nodes[0];
              let next: Source;
              switch (kind) {
                case 'incoming':
                  if (!schema) return;
                  next = { source: kind, path: '', schema: schema.reference };
                  break;
                case 'record':
                  next = {
                    source: kind,
                    path: '',
                    node: first?.key ?? '',
                    outcome: first?.outcomes[0]?.key ?? '',
                  };
                  break;
                case 'node_configuration':
                case 'caller_input':
                case 'predecessor':
                case 'work':
                  next = { source: kind, path: '' };
                  break;
                default:
                  return;
              }
              updateSources({ ...sources, [name]: next });
            }}
          />
          <label>
            {t('factory.sourcePath')}
            <input
              maxLength={1024}
              value={source.path}
              onChange={(event) =>
                updateSources({ ...sources, [name]: { ...source, path: event.target.value } })
              }
            />
          </label>
          {source.source === 'incoming' ? (
            <SelectMenu
              ariaLabel={t('factory.sourceSchema')}
              value={String(
                schemas.findIndex((schema) => schema.reference.digest === source.schema.digest),
              )}
              options={schemas.map((schema, index) => ({
                value: String(index),
                label: `${schema.reference.identity} · ${schema.reference.version}`,
              }))}
              onValueChange={(index) => {
                const schema = schemas[Number(index)];
                if (schema)
                  updateSources({ ...sources, [name]: { ...source, schema: schema.reference } });
              }}
            />
          ) : null}
          {source.source === 'record' ? (
            <>
              <SelectMenu
                ariaLabel={t('factory.sourceNode')}
                value={source.node}
                options={root.nodes.map((node) => ({ value: node.key, label: node.key }))}
                onValueChange={(key) =>
                  updateSources({
                    ...sources,
                    [name]: {
                      ...source,
                      node: key,
                      outcome: root.nodes.find((node) => node.key === key)?.outcomes[0]?.key ?? '',
                    },
                  })
                }
              />
              <SelectMenu
                ariaLabel={t('factory.transitionOutcome')}
                value={source.outcome}
                options={(root.nodes.find((node) => node.key === source.node)?.outcomes ?? []).map(
                  (outcome) => ({ value: outcome.key, label: outcome.key }),
                )}
                onValueChange={(outcome) =>
                  updateSources({ ...sources, [name]: { ...source, outcome } })
                }
              />
            </>
          ) : null}
          {source.source === 'record' || source.source === 'predecessor' ? (
            <SelectMenu
              ariaLabel={t('factory.recordView')}
              value={source.view ?? 'payload'}
              options={[
                { value: 'payload', label: t('factory.payload') },
                { value: 'metadata', label: t('factory.metadata') },
              ]}
              onValueChange={(view) => {
                if (view === 'payload' || view === 'metadata')
                  updateSources({ ...sources, [name]: { ...source, view } });
              }}
            />
          ) : null}
          <button
            type="button"
            onClick={() =>
              updateSources(
                Object.fromEntries(Object.entries(sources).filter(([key]) => key !== name)),
              )
            }
          >
            {t('factory.removeInputSource')}
          </button>
        </fieldset>
      ))}
      <h4>{t('factory.contextEntries')}</h4>
      <button
        type="button"
        disabled={Object.keys(entries).length >= 64 || Object.hasOwn(entries, '')}
        onClick={() =>
          updateEntries({
            ...entries,
            '': {
              source_kind: 'bounded_summary',
              source: { value: { mapping: 'pointer', path: '' } },
            },
          })
        }
      >
        {t('factory.addContextEntry')}
      </button>
      {editableEntries.map(([name, entry], index) => (
        <fieldset key={entryKeys[index]}>
          <label>
            {t('factory.contextEntryName')}
            <input
              maxLength={128}
              value={name}
              onChange={(event) => updateEntries(rename(entries, name, event.target.value))}
            />
          </label>
          <SelectMenu
            ariaLabel={t('factory.contextKind')}
            value={entry.source_kind}
            options={Object.entries(CONTEXT).map(([kind, label]) => ({
              value: kind,
              label: t(label),
            }))}
            onValueChange={(kind) => {
              if (Object.hasOwn(CONTEXT, kind))
                updateEntries({
                  ...entries,
                  [name]: { ...entry, source_kind: kind as keyof typeof CONTEXT },
                });
            }}
          />
          <ValueMappingEditor
            value={entry.source.value}
            onChange={(value) =>
              updateEntries({ ...entries, [name]: { ...entry, source: { value } } })
            }
          />
          <button
            type="button"
            onClick={() =>
              updateEntries(
                Object.fromEntries(Object.entries(entries).filter(([key]) => key !== name)),
              )
            }
          >
            {t('factory.removeContextEntry')}
          </button>
        </fieldset>
      ))}
    </section>
  );
}
function rename<T>(values: Record<string, T>, old: string, name: string): Record<string, T> {
  if (name !== old && Object.hasOwn(values, name)) return values;
  return Object.fromEntries(
    Object.entries(values).map(([key, value]) => [key === old ? name : key, value]),
  );
}
