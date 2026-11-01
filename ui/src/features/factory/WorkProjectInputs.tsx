import { useEffect, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { projectDocumentsApi } from '../../api/projectDocuments';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { ProjectDocumentPublisher } from '../../shared/ProjectDocumentPublisher';
import { formatTimestamp } from '../../shared/display';
import { ProjectDocumentPreview } from '../../shared/ProjectDocumentPreview';
import { useDeadline } from '../../shared/useDeadline';
import { catalogLabels } from './catalogLabels';
type Document = components['schemas']['ProjectDocumentResource'];
export type WorkProjectSelection = {
  task: Document | null;
  acceptance: Document | null;
  specifications: Document[];
};
/** Selects existing Project inputs. Publication and Work admission remain separate confirmed commands. */
export function WorkProjectInputs({
  projectId,
  value,
  onChange,
  onValidityChange,
}: {
  projectId: string;
  value: WorkProjectSelection;
  onChange: (value: WorkProjectSelection) => void;
  onValidityChange: (ready: boolean) => void;
}) {
  const { t, locale } = usePresentation();
  const [managing, setManaging] = useState(false);
  const catalog = useCursorPage(
    queryKeys.projectInputDocuments(projectId),
    async (cursor, signal) => {
      const page = await projectDocumentsApi.list(projectId, cursor, signal);
      return {
        ...page,
        items: page.items.map((item) => ({ ...item, id: item.artifact.artifact_id })),
      };
    },
  );
  const items =
    catalog.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.project_id === projectId) ?? [];
  const publicationLabel = (item: Document) =>
    `${item.name} · ${formatTimestamp(item.published_at, locale).display} · ${item.artifact.content_digest.slice(0, 8)}`;
  const documentLabel = (item: Document) => {
    const others = items.filter(
      (other) => other.name === item.name && other.id !== item.artifact.artifact_id,
    );
    if (!others.length) return item.name;
    const label = publicationLabel(item);
    // Put distinct publication identities first so tail truncation cannot hide them.
    return others.some((other) => publicationLabel(other) === label)
      ? `${item.artifact.artifact_id} · ${label}`
      : label;
  };
  const selected = [value.task, value.acceptance, ...value.specifications].filter(
    (item): item is Document => !!item,
  );
  const current = (item: Document) =>
    items.some(
      (row) =>
        row.id === item.artifact.artifact_id &&
        row.artifact.content_digest === item.artifact.content_digest &&
        row.artifact.encoded_size === item.artifact.encoded_size &&
        row.expires_at > Date.now(),
    );
  const valid = !!value.task && !!value.acceptance && !catalog.error && selected.every(current);
  const expiry = selected.length
    ? Math.min(
        ...selected.map(
          (item) =>
            items.find((row) => row.id === item.artifact.artifact_id)?.expires_at ??
            item.expires_at,
        ),
      )
    : null;
  const validity = (
    <InputValidity
      key={expiry ?? 'none'}
      ready={valid}
      expiresAt={expiry}
      onChange={onValidityChange}
    />
  );
  if (managing)
    return (
      <section>
        {validity}
        <button type="button" onClick={() => setManaging(false)}>
          {t('factory.backToManualInput')}
        </button>
        <ProjectDocumentPublisher projectId={projectId} api={projectDocumentsApi} />
      </section>
    );
  const max = FACTORY_AUTHORING_SCHEMAS.FactoryWorkArtifacts.properties.specifications.maxItems;
  const removeSpecification = (id: string) =>
    onChange({
      ...value,
      specifications: value.specifications.filter((doc) => doc.artifact.artifact_id !== id),
    });
  return (
    <fieldset>
      <legend>{t('factory.manualInputDocuments')}</legend>
      {validity}
      <p>{t('factory.manualInputDocumentsHelp')}</p>
      <button type="button" onClick={() => setManaging(true)}>
        {t('projectDocument.manage')}
      </button>
      <PagedExplorerBranch
        query={catalog}
        labels={catalogLabels(t('factory.manualInputDocuments'))}
        empty={t('projectDocument.empty')}
      >
        {() => (
          <>
            {(['task', 'acceptance'] as const).map((kind) => (
              <fieldset key={kind}>
                <legend>
                  {t(kind === 'task' ? 'factory.taskDocument' : 'factory.criteriaDocument')}
                </legend>
                <SelectMenu
                  ariaLabel={t(
                    kind === 'task' ? 'factory.taskDocument' : 'factory.criteriaDocument',
                  )}
                  value={value[kind]?.artifact.artifact_id ?? ''}
                  options={[
                    { value: '', label: t('projectDocument.choose') },
                    ...items.map((item) => ({ value: item.id, label: documentLabel(item) })),
                    ...(value[kind] && !current(value[kind])
                      ? [
                          {
                            value: value[kind].artifact.artifact_id,
                            label: t('factory.inputDocumentUnavailable'),
                          },
                        ]
                      : []),
                  ]}
                  onValueChange={(id) =>
                    onChange({
                      ...value,
                      [kind]:
                        items.find((item) => item.id === id && item.expires_at > Date.now()) ??
                        null,
                    })
                  }
                />
                {value[kind] ? (
                  <ProjectDocumentPreview
                    key={value[kind].artifact.artifact_id}
                    api={projectDocumentsApi}
                    projectId={projectId}
                    document={value[kind]}
                    label={t('factory.previewDocument')}
                  />
                ) : null}
              </fieldset>
            ))}
            <fieldset>
              <legend>{t('factory.specificationArtifacts')}</legend>
              {items.map((item) => (
                <label key={item.id}>
                  <input
                    type="checkbox"
                    checked={value.specifications.some(
                      (doc) => doc.artifact.artifact_id === item.id,
                    )}
                    disabled={
                      !value.specifications.some((doc) => doc.artifact.artifact_id === item.id) &&
                      (item.expires_at <= Date.now() || value.specifications.length >= max)
                    }
                    onChange={(e) =>
                      e.target.checked
                        ? onChange({ ...value, specifications: [...value.specifications, item] })
                        : removeSpecification(item.id)
                    }
                  />
                  {documentLabel(item)}
                </label>
              ))}
            </fieldset>
          </>
        )}
      </PagedExplorerBranch>
      {/* Retained selections stay removable even when their catalogue entry is unavailable. */}
      <ul>
        {value.specifications.map((item) => (
          <li key={item.artifact.artifact_id}>
            <span>{item.name}</span>
            <button type="button" onClick={() => removeSpecification(item.artifact.artifact_id)}>
              {t('factory.removeDocument', { name: item.name })}
            </button>
          </li>
        ))}
      </ul>
      {selected.some((item) => !current(item)) ? (
        <p role="alert">{t('factory.inputDocumentUnavailable')}</p>
      ) : null}
    </fieldset>
  );
}

/** Deadline identity resets only the timer, never the selected documents. */
function InputValidity({
  ready,
  expiresAt,
  onChange,
}: {
  ready: boolean;
  expiresAt: number | null;
  onChange: (ready: boolean) => void;
}) {
  const { t } = usePresentation();
  const expired = useDeadline(expiresAt);
  useEffect(() => {
    onChange(ready && !expired);
  }, [ready, expired, onChange]);
  return ready && expired ? <p role="alert">{t('factory.inputDocumentUnavailable')}</p> : null;
}
