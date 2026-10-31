import type { components } from '../../../.generated/api/schema';
import { useState } from 'react';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import type { ProjectDocumentsApi } from '../../api/projectDocuments';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { SelectMenu } from '../../shared/SelectMenu';
import { ProjectDocumentRead } from '../../shared/ProjectDocumentReader';
import { ProjectDocumentPublisher } from '../../shared/ProjectDocumentPublisher';
import { catalogLabels } from './catalogLabels';
import {
  documentSelections,
  removeDocument,
  selectDocument,
  setDocumentRole,
} from './flowDocumentBindings';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
export type FlowDocumentServices = { api: ProjectDocumentsApi; projectId: string };

/** Exact per-step documents remain ordinary Context bindings, independent of executor selection. */
export function FlowDocuments({
  node,
  services,
  onChange,
  processManaged = false,
}: {
  node: Node;
  services: FlowDocumentServices;
  onChange: (node: Node) => void;
  processManaged?: boolean;
}) {
  const { t } = usePresentation();
  const { api, projectId } = services;
  const [managing, setManaging] = useState(false);
  const catalog = useCursorPage(
    queryKeys.projectInputDocuments(projectId),
    async (cursor, signal) => {
      const page = await api.list(projectId, cursor, signal);
      return {
        ...page,
        items: page.items
          .filter((item) => item.project_id === projectId)
          .map((item) => ({ ...item, id: item.artifact.artifact_id })),
      };
    },
  );
  const documents = catalog.data?.pages.flatMap((page) => page.items) ?? [];
  const selected = documentSelections(node, processManaged);
  const entries = node.context_binding?.entries ?? {};
  const requirements = [
    ...new Set(
      Object.values(node.profile_selection?.candidates ?? {}).flatMap(
        (candidate) => candidate.profile.definition.context_requirements,
      ),
    ),
  ];
  const missing = requirements.filter(
    (name) => !entries[name] || entries[name].when_present != null,
  );
  const count =
    Object.keys(node.context_binding?.entries ?? {}).length +
    Object.keys(node.context_binding?.artifact_collections ?? {}).length;
  const maximum =
    FACTORY_AUTHORING_SCHEMAS.FactoryFlowContextBinding.properties.entries.maxProperties;
  if (managing)
    return (
      <section aria-label={t('projectDocument.manage')}>
        <button type="button" onClick={() => setManaging(false)}>
          {t('factory.returnToStep')}
        </button>
        <ProjectDocumentPublisher api={api} projectId={projectId} />
      </section>
    );
  return (
    <fieldset>
      <legend>{t('factory.documents')}</legend>
      {missing.length ? <p role="alert">{t('factory.profileContextRequired')}</p> : null}
      <button type="button" onClick={() => setManaging(true)}>
        {t('projectDocument.manage')}
      </button>
      <PagedExplorerBranch
        query={catalog}
        empty={t('projectDocument.empty')}
        labels={catalogLabels(t('factory.documents'))}
      >
        {() =>
          documents.map((document) => {
            const entry = selected.find((item) => item.artifact.artifact_id === document.id);
            return (
              <label key={document.id}>
                <input
                  type="checkbox"
                  checked={!!entry}
                  disabled={
                    catalog.isError ||
                    (!entry && (count >= maximum || document.expires_at <= Date.now()))
                  }
                  onChange={(event) =>
                    onChange(
                      event.target.checked
                        ? selectDocument(node, document.artifact, 'specification', processManaged)
                        : entry
                          ? removeDocument(node, entry.key)
                          : node,
                    )
                  }
                />
                {document.name}
              </label>
            );
          })
        }
      </PagedExplorerBranch>
      <ul>
        {selected.map((entry) => {
          const document = documents.find(
            (item) =>
              item.id === entry.artifact.artifact_id &&
              item.artifact.content_digest === entry.artifact.content_digest &&
              item.artifact.encoded_size === entry.artifact.encoded_size,
          );
          const name = document?.name ?? t('factory.selectedDocument');
          return (
            <li key={entry.key} aria-label={name}>
              <span>{name}</span>
              {!document ? <p>{t('factory.documentMetadataNotLoaded')}</p> : null}
              <SelectMenu
                ariaLabel={t('factory.documentRole', { name })}
                value={entry.selection.source_kind}
                options={[
                  { value: 'policy', label: t('factory.documentInstructions') },
                  { value: 'specification', label: t('factory.documentReference') },
                ]}
                onValueChange={(kind) => {
                  if (kind === 'policy' || kind === 'specification')
                    onChange(setDocumentRole(node, entry.key, kind));
                }}
              />
              <button type="button" onClick={() => onChange(removeDocument(node, entry.key))}>
                {t('factory.removeDocument', { name })}
              </button>
              <DocumentPreview document={document} artifact={entry.artifact} services={services} />
              {requirements.length ? (
                <details>
                  <summary>{t('factory.documentInputDetails')}</summary>
                  <SelectMenu
                    ariaLabel={t('factory.documentProfileInput', { name })}
                    value={requirements.includes(entry.key) ? entry.key : ''}
                    options={[
                      { value: '', label: t('factory.additionalDocument') },
                      ...requirements
                        .filter((key) => key === entry.key || !entries[key])
                        .map((key) => ({ value: key, label: key })),
                    ]}
                    onValueChange={(input) => {
                      const key = input || `project-document.${entry.artifact.artifact_id}`;
                      if (key === entry.key || entries[key]) return;
                      const renamed = { ...entries };
                      delete renamed[entry.key];
                      renamed[key] = entry.selection;
                      onChange({
                        ...node,
                        context_binding: { ...node.context_binding, entries: renamed },
                      });
                    }}
                  />
                </details>
              ) : null}
            </li>
          );
        })}
      </ul>
    </fieldset>
  );
}

function DocumentPreview({
  document,
  artifact,
  services,
}: {
  document: components['schemas']['ProjectDocumentResource'] | undefined;
  artifact: components['schemas']['FactoryArtifactReference'];
  services: FlowDocumentServices;
}) {
  const { t } = usePresentation();
  const [preview, setPreview] = useState('');
  const [reading, setReading] = useState(false);
  return (
    <>
      <ProjectDocumentRead
        api={services.api}
        projectId={services.projectId}
        document={document ?? { artifact, name: t('factory.selectedDocument') }}
        label={t('factory.previewDocument')}
        disabled={reading}
        onLoad={(value) => setPreview(value.content)}
        onLoading={setReading}
      />
      {preview ? <pre>{preview}</pre> : null}
    </>
  );
}
