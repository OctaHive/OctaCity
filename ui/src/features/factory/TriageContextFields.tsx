import { useRef, useState } from 'react';
import { FACTORY_SETUP_SCHEMAS } from '../../../.generated/api/factory-setup';
import type { ProjectDocumentsApi } from '../../api/projectDocuments';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { catalogLabels } from './catalogLabels';
import type { FactoryWorkSources } from './FactoryWorkForm';

type Context = components['schemas']['TriageProjectContextDraft'];
/** Each stage selects exact retained Project documents independently; no Build history is required. */
export function TriageContextFields({
  projectId,
  api,
  value,
  onChange,
}: {
  projectId: string;
  api: ProjectDocumentsApi;
  value: Context | undefined;
  onChange: (value: Context) => void;
}) {
  const { t } = usePresentation();
  const [names, setNames] = useState<Record<string, string>>({});
  const catalog = useCursorPage(
    queryKeys.projectInputDocuments(projectId),
    async (cursor, signal) => {
      const page = await api.list(projectId, cursor, signal);
      return {
        ...page,
        items: page.items.map((document) => ({ ...document, id: document.artifact.artifact_id })),
      };
    },
  );
  const documents =
    catalog.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.project_id === projectId) ?? [];
  const context = value ?? { project_id: projectId, goals: null, rules: null, documents: {} };
  return (
    <fieldset>
      <legend>{t('triage.context')}</legend>
      <PagedExplorerBranch
        query={catalog}
        empty={t('triage.noDocuments')}
        labels={catalogLabels(t('triage.context'))}
      >
        {(items) => <p>{t('triage.documentsLoaded', { count: items.length })}</p>}
      </PagedExplorerBranch>
      {(['goals', 'rules'] as const).map((field) => {
        const selected = context[field];
        const options = documents.map((item) => ({
          value: item.artifact.artifact_id,
          label: item.name,
        }));
        if (selected && !options.some((item) => item.value === selected.artifact_id))
          options.push({
            value: selected.artifact_id,
            label: names[selected.artifact_id] ?? t('triage.retainedDocument'),
          });
        return (
          <label key={field}>
            <span>{t(field === 'goals' ? 'triage.goals' : 'triage.rules')}</span>
            <SelectMenu
              ariaLabel={t(field === 'goals' ? 'triage.goals' : 'triage.rules')}
              value={selected?.artifact_id ?? ''}
              options={[{ value: '', label: t('factory.none') }, ...options]}
              onValueChange={(id) => {
                const item = documents.find((document) => document.artifact.artifact_id === id);
                if (item && !catalog.isError) {
                  setNames({ ...names, [id]: item.name });
                  onChange({
                    ...context,
                    [field]: item.artifact,
                  });
                } else if (!id) onChange({ ...context, [field]: null });
              }}
            />
          </label>
        );
      })}
      <SupportingDocuments
        documents={documents}
        context={context}
        ready={!catalog.isError}
        onChange={onChange}
      />
    </fieldset>
  );
}

function SupportingDocuments({
  documents,
  context,
  ready,
  onChange,
}: {
  documents: components['schemas']['ProjectDocumentResource'][];
  context: Context;
  ready: boolean;
  onChange: (value: Context) => void;
}) {
  const { t } = usePresentation();
  const [alias, setAlias] = useState('');
  const [selected, setSelected] = useState('');
  const bounds = FACTORY_SETUP_SCHEMAS.TriageProjectContextDraft.properties.documents;
  const document = documents.find((item) => item.artifact.artifact_id === selected);
  const canAdd =
    ready &&
    !!document &&
    new RegExp(bounds.propertyNames.pattern).test(alias) &&
    alias.length <= bounds.propertyNames.maxLength &&
    !Object.hasOwn(context.documents, alias) &&
    Object.keys(context.documents).length < bounds.maxProperties;
  return (
    <fieldset>
      <legend>{t('triage.supportingDocuments')}</legend>
      <label>
        {t('triage.documentAlias')}
        <input
          value={alias}
          maxLength={bounds.propertyNames.maxLength}
          pattern={bounds.propertyNames.pattern}
          onChange={(event) => setAlias(event.target.value)}
        />
      </label>
      <label>
        <span>{t('triage.supportingDocument')}</span>
        <SelectMenu
          ariaLabel={t('triage.supportingDocument')}
          value={selected}
          options={[
            { value: '', label: t('factory.none') },
            ...documents.map((item) => ({ value: item.artifact.artifact_id, label: item.name })),
          ]}
          onValueChange={setSelected}
        />
      </label>
      <button
        type="button"
        disabled={!canAdd}
        onClick={() => {
          if (!canAdd || !document) return;
          onChange({ ...context, documents: { ...context.documents, [alias]: document.artifact } });
          setAlias('');
          setSelected('');
        }}
      >
        {t('triage.addDocument')}
      </button>
      <ul>
        {Object.entries(context.documents).map(([name, reference]) => (
          <li key={name}>
            {name} ·{' '}
            {documents.find((item) => item.artifact.artifact_id === reference.artifact_id)?.name ??
              t('triage.retainedDocument')}
            <button
              type="button"
              aria-label={t('triage.removeDocument', { name })}
              onClick={() =>
                onChange({
                  ...context,
                  documents: Object.fromEntries(
                    Object.entries(context.documents).filter(([key]) => key !== name),
                  ),
                })
              }
            >
              {t('triage.removeDocument', { name })}
            </button>
          </li>
        ))}
      </ul>
    </fieldset>
  );
}
/** The code Repository is independent of native task containers and never follows a refreshed version. */
export function TriageCodeRepository({
  projectId,
  sources,
  value,
  onChange,
}: {
  projectId: string;
  sources: FactoryWorkSources;
  value:
    NonNullable<components['schemas']['FactoryPresetResolutionRequest']['repository']> | undefined;
  onChange: (
    value: NonNullable<components['schemas']['FactoryPresetResolutionRequest']['repository']>,
  ) => void;
}) {
  const { t } = usePresentation();
  const [error, setError] = useState<Error | null>(null);
  const latestSelectionRef = useRef(0);
  const repositories = useCursorPage(
    queryKeys.factoryWorkSources(projectId, 'repositories'),
    (cursor, signal) => sources.listRepositories(projectId, cursor, signal),
  );
  const items =
    repositories.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.project_id === projectId) ?? [];
  async function select(id: string) {
    const selected = items.find((item) => item.id === id);
    if (!selected) return;
    const selection = ++latestSelectionRef.current;
    try {
      const details = await sources.getRepository(selected.id, selected.version);
      if (selection !== latestSelectionRef.current) return;
      if (
        details.project_id === projectId &&
        details.id === selected.id &&
        details.version === selected.version &&
        details.definition_digest
      )
        onChange({
          id: details.id,
          version: details.version,
          project_id: projectId,
          definition_digest: details.definition_digest,
        });
      setError(null);
    } catch (failure) {
      if (selection === latestSelectionRef.current)
        setError(failure instanceof Error ? failure : new Error());
    }
  }
  return (
    <>
      <PagedExplorerBranch
        query={repositories}
        empty={t('triage.repositoryMissing')}
        labels={catalogLabels(t('triage.repository'))}
      >
        {() => (
          <label>
            <span>{t('triage.repository')}</span>
            <SelectMenu
              ariaLabel={t('triage.repository')}
              value={value ? `${value.id}:${value.version}` : ''}
              options={[
                { value: '', label: t('factory.none') },
                ...items.map((item) => ({
                  value: `${item.id}:${item.version}`,
                  label: `${item.name} · v${item.version}`,
                })),
              ]}
              onValueChange={(key) => {
                const item = items.find((entry) => `${entry.id}:${entry.version}` === key);
                if (item) void select(item.id);
              }}
            />
          </label>
        )}
      </PagedExplorerBranch>
      {error ? (
        <QueryFailureNotice
          title={t('triage.repository')}
          error={error}
          onRetry={() => setError(null)}
        />
      ) : null}
    </>
  );
}
