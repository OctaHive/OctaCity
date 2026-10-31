import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { catalogLabels } from './catalogLabels';
import type { SourceRepositories } from './SourceInputEditor';
import { CodeScope } from './CodeScope';

type S = components['schemas'];
export type CodeResourceServices = { projectId: string; repositories: SourceRepositories };
const sourceLimit =
  FACTORY_AUTHORING_SCHEMAS.FactoryFlowDefinition.properties.code_sources.anyOf[0].maxProperties;
export function useCodeRepository(
  services: CodeResourceServices,
  selected?: { id: string; version: number },
) {
  return useQuery({
    queryKey: queryKeys.repository(selected?.id ?? '', selected?.version ?? 0),
    enabled: !!selected,
    queryFn: ({ signal }) =>
      services.repositories.getRepository(selected!.id, selected!.version, signal),
  });
}
/** Inventory is a ceiling, never an implicit input or permission grant. Exact versions survive catalogue refreshes. */
export function FlowCodeSources({
  root,
  services,
  onChange,
}: {
  root: S['FactoryFlowRootDraft'];
  services: CodeResourceServices;
  onChange: (root: S['FactoryFlowRootDraft']) => void;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<S['RepositorySummaryResource']>();
  const [selection, setSelection] = useState<S['ManualSource']>();
  const catalog = useCursorPage(
    queryKeys.factoryWorkSources(services.projectId, 'repositories'),
    (cursor, signal) => services.repositories.listRepositories(services.projectId, cursor, signal),
  );
  const detail = useCodeRepository(services, selected);
  const repository = detail.data;
  const exact =
    !detail.error &&
    !catalog.error &&
    repository &&
    selected &&
    repository.id === selected.id &&
    repository.version === selected.version &&
    repository.project_id === services.projectId &&
    repository.definition_digest
      ? repository
      : undefined;
  const sources = root.code_sources ?? {};
  const policy = exact?.definition.selection;
  const chosen =
    selection ?? (policy?.default_reference ? { kind: 'default_reference' as const } : undefined);
  const add = () => {
    if (!exact || !chosen || !exact.definition_digest) return;
    let ordinal = 1;
    while (Object.hasOwn(sources, `code_${ordinal}`)) ordinal++;
    onChange({
      ...root,
      code_sources: {
        ...sources,
        [`code_${ordinal}`]: {
          repository: {
            id: exact.id,
            version: exact.version,
            project_id: exact.project_id,
            definition_digest: exact.definition_digest,
          },
          selection: chosen,
          access: 'read_only',
          scope: { kind: 'repository' },
        },
      },
    });
    setSelected(undefined);
    setSelection(undefined);
  };
  return (
    <fieldset>
      <legend>{t('factory.availableCode')}</legend>
      <p>{t('factory.availableCodeHelp')}</p>
      {Object.entries(sources).map(([alias, source]) => (
        <CodeSourceRow
          key={alias}
          alias={alias}
          source={source}
          root={root}
          services={services}
          onChange={onChange}
        />
      ))}
      <PagedExplorerBranch
        query={catalog}
        empty={t('factory.noCodeRepositories')}
        labels={catalogLabels(t('factory.projectRepository'))}
      >
        {(items) => (
          <SelectMenu
            ariaLabel={t('factory.projectRepository')}
            value={selected ? `${selected.id}:${selected.version}` : ''}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...items
                .filter((item) => item.project_id === services.projectId)
                .map((item) => ({
                  value: `${item.id}:${item.version}`,
                  label: `${item.name} · v${item.version}`,
                })),
            ]}
            onValueChange={(key) => {
              setSelected(items.find((item) => `${item.id}:${item.version}` === key));
              setSelection(undefined);
            }}
          />
        )}
      </PagedExplorerBranch>
      {detail.error ? (
        <QueryFailureNotice
          title={t('factory.projectRepository')}
          error={detail.error}
          onRetry={() => void detail.refetch()}
        />
      ) : null}
      {repository && !exact ? <p role="alert">{t('factory.repositoryDigestMissing')}</p> : null}
      {policy ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.codeSelection')}
            value={chosen?.kind === 'reference' ? `ref:${chosen.value}` : (chosen?.kind ?? '')}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...(policy.default_reference
                ? [
                    {
                      value: 'default_reference',
                      label: `${t('manualBuild.defaultReference')} · ${policy.default_reference}`,
                    },
                  ]
                : []),
              ...policy.allowed_references.map((value) => ({
                value: `ref:${value}`,
                label: value,
              })),
              ...(policy.allow_exact_revision
                ? [{ value: 'exact_revision', label: t('manualBuild.exactRevision') }]
                : []),
            ]}
            onValueChange={(key) =>
              setSelection(
                key === 'default_reference'
                  ? { kind: 'default_reference' }
                  : key === 'exact_revision'
                    ? { kind: 'exact_revision', value: '' }
                    : key.startsWith('ref:')
                      ? { kind: 'reference', value: key.slice(4) }
                      : undefined,
              )
            }
          />
          {selection?.kind === 'exact_revision' ? (
            <label>
              {t('manualBuild.exactRevision')}
              <input
                value={selection.value}
                onChange={(e) => setSelection({ kind: 'exact_revision', value: e.target.value })}
              />
            </label>
          ) : null}
        </>
      ) : null}
      <button
        type="button"
        disabled={
          !exact ||
          !chosen ||
          ('value' in chosen && !chosen.value) ||
          Object.keys(sources).length >= sourceLimit
        }
        onClick={add}
      >
        {t('factory.addCodeSource')}
      </button>
    </fieldset>
  );
}
function CodeSourceRow({
  alias,
  source,
  root,
  services,
  onChange,
}: {
  alias: string;
  source: S['FactoryFlowCodeSource'];
  root: S['FactoryFlowRootDraft'];
  services: CodeResourceServices;
  onChange: (root: S['FactoryFlowRootDraft']) => void;
}) {
  const { t } = usePresentation();
  const query = useCodeRepository(services, source.repository);
  const name = query.data?.name ?? alias;
  const users = root.nodes.filter(
    (node) => node.code?.kind === 'source' && node.code.source === alias,
  );
  const change = (patch: Partial<S['FactoryFlowCodeSource']>) =>
    onChange({ ...root, code_sources: { ...root.code_sources, [alias]: { ...source, ...patch } } });
  return (
    <fieldset>
      <legend>
        {name} · v{source.repository.version}
      </legend>
      {query.error ? <p role="alert">{t('factory.codeMetadataUnavailable')}</p> : null}
      <p>
        {source.selection.kind === 'default_reference'
          ? t('factory.codeAdmittedRevision')
          : source.selection.value}
      </p>
      <SelectMenu
        ariaLabel={t('factory.codeCeilingAccess', { name })}
        value={source.access}
        options={[
          { value: 'read_only', label: t('factory.codeRead') },
          { value: 'read_write', label: t('factory.codeWrite') },
        ]}
        onValueChange={(access) => {
          if (access === 'read_only' || access === 'read_write') change({ access });
        }}
      />
      <CodeScope value={source.scope} onChange={(scope) => change({ scope })} />
      {users.length ? (
        <p>
          {t('factory.codeUsedBy', {
            steps: users.map((node) => node.key || t('factory.unnamedNode')).join(', '),
          })}
        </p>
      ) : null}
      <button
        type="button"
        disabled={!!users.length}
        onClick={() => {
          const next = { ...root.code_sources };
          delete next[alias];
          onChange({ ...root, code_sources: next });
        }}
      >
        {t('factory.removeCodeSource', { name })}
      </button>
      <details>
        <summary>{t('factory.routeDetails')}</summary>
        <p>
          {alias} · {source.repository.id} · {source.repository.definition_digest}
        </p>
      </details>
    </fieldset>
  );
}
