import { useRef, useState, type RefObject } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import {
  safeSourceError,
  sourceFailure,
  sourceNeedsRefresh,
  sourcePreviewFailure,
} from '../../shared/sourceErrors';
import { sourceBindingKey } from '../../shared/sourceReferences';
import type { components } from '../../../.generated/api/schema';
import type { IssueSourcesApi, IssueSourceImport } from './sourceApi';
import type { IssueSourceImportIntent } from './sourceApi';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import { importPredicateSummary } from './importPredicateSummary';

type Preview = components['schemas']['ProjectIssueSourcePreview'];
/** Preview bodies stay in the mounted form, never in query state or browser storage. */
export function ProjectSourceEditor({
  record,
  projectId,
  api,
  connections,
  ready,
  onClose,
  onRefresh,
  fallbackFocusRef,
}: {
  record?: IssueSourceImport | undefined;
  projectId: string;
  api: IssueSourcesApi;
  connections: components['schemas']['IntegrationConnection'][];
  ready: boolean;
  onClose: () => void;
  onRefresh: () => Promise<unknown>;
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [id] = useState(() => record?.id ?? crypto.randomUUID());
  const [enabled, setEnabled] = useState(record?.definition.enabled ?? false);
  const [predicate] = useState(record?.definition.predicate);
  const [published, setPublished] = useState(false);
  const [review, setReview] = useState<{
    intent: IssueSourceImportIntent;
    summary: string;
    returnFocus: HTMLElement;
  } | null>(null);
  const [connectionId, setConnectionId] = useState(record?.definition.connection.id ?? '');
  const [profile, setProfile] = useState(() =>
    record ? sourceBindingKey(record.definition.source) : '',
  );
  const connection = connections.find((item) => item.id === connectionId);
  const choices = useQuery({
    queryKey: queryKeys.issueImportChoices(projectId, connectionId, connection?.version ?? 0),
    queryFn: ({ signal }) =>
      api.importChoices(projectId, connectionId, connection!.version, signal),
    enabled: !!connection,
    gcTime: 0,
  });
  const readers = choices.data?.items.filter((item) => item.purpose === 'task_read') ?? [];
  const selected = readers.find((item) => sourceBindingKey(item.binding) === profile);
  const signature = JSON.stringify([
    connectionId,
    connection?.version,
    choices.data?.access_version,
    selected?.binding,
  ]);
  const latestRef = useRef(signature);
  latestRef.current = signature;
  const [preview, setPreview] = useState<{ signature: string; data: Preview } | null>(null);
  const [error, setError] = useState<{
    failure: ReturnType<typeof safeSourceError>;
    refresh: boolean;
  } | null>(null);
  const [pending, setPending] = useState(false);
  // Refresh changed parents before preview or publication can use the draft again.
  const refreshRequired = error?.refresh ?? false;
  const intent: IssueSourceImportIntent | null =
    connection &&
    selected &&
    choices.data &&
    ready &&
    !choices.error &&
    !choices.isFetching &&
    !refreshRequired
      ? {
          project_id: projectId,
          id,
          expected_current_version: record?.version ?? null,
          definition: {
            ...(predicate ? { predicate } : {}),
            enabled,
            source: selected.binding,
            connection: {
              id: connection.id,
              version: connection.version,
              access_version: choices.data.access_version,
              source_configuration: selected.connection_configuration,
              reporter_configuration: null,
            },
          },
        }
      : null;
  async function execute(attempt: ConfirmedMutationAttempt<IssueSourceImportIntent>) {
    try {
      const result = await api.publishImport(
        structuredClone(attempt.request) as IssueSourceImportIntent,
        attempt.headers,
      );
      setPublished(true);
      return result;
    } catch (failure) {
      throw sourceFailure(failure, t);
    }
  }
  async function read() {
    if (pending) return;
    const captured = signature;
    setPending(true);
    try {
      if (refreshRequired) {
        await onRefresh();
        await choices.refetch({ throwOnError: true });
        setError(null);
        return;
      }
      if (!connection || !selected || !choices.data || choices.error || choices.isFetching) return;
      setError(null);
      setPreview(null);
      const data = await api.previewImport(projectId, {
        connection_id: connection.id,
        connection_version: connection.version,
        access_version: choices.data.access_version,
        source: selected.binding,
        limit: 10,
      });
      if (latestRef.current === captured) setPreview({ signature: captured, data });
    } catch (failure) {
      if (refreshRequired || latestRef.current === captured) {
        setError({
          failure: safeSourceError(failure),
          refresh: refreshRequired || sourceNeedsRefresh(failure),
        });
      }
    } finally {
      setPending(false);
    }
  }
  return (
    <fieldset>
      <legend>{t(record ? 'issueImport.edit' : 'issueImport.add')}</legend>
      <SelectMenu
        ariaLabel={t('factory.sourceConnection')}
        value={connectionId}
        options={[
          { value: '', label: t('factory.chooseInput') },
          ...connections.map((item) => ({ value: item.id, label: item.definition.name })),
        ]}
        onValueChange={(value) => {
          setConnectionId(value);
          setProfile('');
          setError(null);
        }}
      />
      {choices.error ? (
        <QueryFailureNotice
          title={t('issueSource.profile')}
          error={sourceFailure(choices.error, t)}
          onRetry={() => choices.refetch()}
        />
      ) : null}
      <SelectMenu
        ariaLabel={t('issueSource.profile')}
        disabled={refreshRequired}
        value={profile}
        options={[
          { value: '', label: t('issueSource.chooseProfile') },
          ...readers.map((item) => ({
            value: sourceBindingKey(item.binding),
            label: `${item.locator} · ${item.binding.configuration.identity} / ${item.binding.configuration.version}`,
          })),
        ]}
        onValueChange={(value) => {
          setProfile(value);
          setError(null);
        }}
      />
      {connection && choices.isSuccess && !readers.length ? (
        <p>{t('factory.noSourceAccess')}</p>
      ) : null}
      {predicate ? <p>{importPredicateSummary(predicate, t)}</p> : null}
      <button
        type="button"
        disabled={!selected || choices.isFetching || !!choices.error || pending || refreshRequired}
        onClick={() => void read()}
      >
        {t('issueImport.preview')}
      </button>
      <label>
        <input
          type="checkbox"
          checked={enabled}
          onChange={(event) => setEnabled(event.target.checked)}
        />
        {t('issueImport.enabled')}
      </label>
      <button
        type="button"
        disabled={!intent || pending || review !== null}
        onClick={(event) =>
          intent &&
          setReview({
            intent: structuredClone(intent),
            returnFocus: event.currentTarget,
            summary: `${connection?.definition.name} · v${connection?.version}; ${selected?.locator} · ${selected?.binding.configuration.identity} / ${selected?.binding.configuration.version}; ${importPredicateSummary(intent.definition.predicate, t)}; ${t(enabled ? 'issueImport.active' : 'issueImport.inactive')}. ${t('issueImport.help')}`,
          })
        }
      >
        {t('issueImport.review')}
      </button>
      {error ? (
        <QueryFailureNotice
          title={t('issueImport.preview')}
          error={sourceFailure(error.failure, t)}
          onRetry={read}
        />
      ) : null}
      {preview?.signature === signature ? (
        <section aria-label={t('issueImport.previewTitle')}>
          {preview.data.failure ? (
            <QueryFailureNotice
              title={t('factory.previewUnavailable')}
              error={sourcePreviewFailure(preview.data.failure, preview.data.retry_after_millis, t)}
              onRetry={read}
            />
          ) : (
            <>
              {!preview.data.items.length ? <p>{t('factory.previewEmpty')}</p> : null}
              <ul>
                {preview.data.items.map((item) => (
                  <li key={`${item.item.resource}:${item.item.item}`}>
                    {item.display_id} · {item.title}
                  </li>
                ))}
              </ul>
              {preview.data.has_more ? <p>{t('factory.previewMore')}</p> : null}
            </>
          )}
        </section>
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('issueImport.publish')}
          confirmLabel={t('issueImport.confirm')}
          consequence={review.summary}
          request={review.intent}
          returnFocus={review.returnFocus}
          fallbackFocusRef={fallbackFocusRef}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.issueImports(projectId) }]}
          execute={execute}
          onRefresh={async () => {
            await onRefresh();
            await choices.refetch();
          }}
          onClose={() => {
            setReview(null);
            if (published) onClose();
          }}
          renderSuccess={() => <p>{t('issueImport.published')}</p>}
        />
      ) : null}
    </fieldset>
  );
}
