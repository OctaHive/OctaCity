import { useRef, useState, type RefObject } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceBindingKey } from '../../shared/sourceReferences';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import { IssueSourceProfileSelect } from './IssueSourceProfileSelect';
import type {
  IssueSourceAttachment as Attachment,
  IssueSourceChoice,
  IssueSourcesApi,
  IssueSynchronization,
} from './sourceApi';
import { Link } from 'react-router-dom';
import { IssueSourceAssociationConflict } from './sourceApi';

/** Explicit native selection preserves the reviewed task and never creates a remote ticket. */
export function IssueSourceAttachment({
  issue,
  api,
  review,
  choices,
  ready,
  onReload,
  fallbackFocusRef,
}: {
  issue: Issue;
  api: IssueSourcesApi;
  review: IssueSynchronization;
  choices: readonly IssueSourceChoice[];
  ready: boolean;
  onReload: () => unknown;
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const [baseline] = useState(() => structuredClone(review));
  const [sourceKey, setSourceKey] = useState('');
  const choice = choices.find((c) => sourceBindingKey(c.source) === sourceKey);
  const [native, setNative] = useState('');
  const [retained, setRetained] = useState<{
    page: Awaited<ReturnType<IssueSourcesApi['candidates']>>;
    continuation: string | null;
  } | null>(null);
  const preview = useMutation({
    gcTime: 0,
    mutationFn: (body: Parameters<IssueSourcesApi['candidates']>[2]) =>
      api.candidates(issue.project_id, issue.id, body),
    onSuccess: (page, body) => setRetained({ page, continuation: body.continuation ?? null }),
  });
  const fresh =
    ready &&
    review.issue_version === baseline.issue_version &&
    review.link === null &&
    JSON.stringify(review.field_policy) === JSON.stringify(baseline.field_policy);
  const selected = retained?.page.items.find((item) => item.item === native);
  const [requested, setRequested] = useState<Attachment | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  function discover(next = false) {
    if (!choice || !fresh) return;
    preview.mutate({
      source: choice.source,
      scan_id: next ? (retained?.page.scan_id ?? null) : null,
      continuation: next ? (retained?.page.next_cursor ?? null) : null,
    });
  }
  return (
    <section aria-label={t('issueSource.attachment')}>
      <p>{t('issueSource.attachHelp')}</p>
      {!fresh ? <p role="alert">{t('issueSource.stale')}</p> : null}
      <button type="button" onClick={onReload}>
        {t('issueSource.reload')}
      </button>
      <fieldset disabled={!fresh || preview.isPending}>
        <legend>{t('issueSource.attachment')}</legend>
        <IssueSourceProfileSelect
          choices={choices}
          value={sourceKey}
          onChange={(value) => {
            setSourceKey(value);
            setNative('');
            setRetained(null);
            preview.reset();
          }}
        />
        {!choices.length ? <p>{t('issueSource.noProfiles')}</p> : null}
        <button type="button" disabled={!choice} onClick={() => discover()}>
          {t('issueSource.find')}
        </button>
        {preview.error ? (
          <QueryFailureNotice
            title={t('issueSource.native')}
            error={preview.error}
            onRetry={() => discover()}
          />
        ) : null}
        {retained ? (
          <>
            <SelectMenu
              ariaLabel={t('issueSource.native')}
              value={native}
              options={[
                { value: '', label: t('issueSource.chooseNative') },
                ...retained.page.items.map((item) => ({ value: item.item, label: item.item })),
              ]}
              onValueChange={setNative}
            />
            {!retained.page.items.length ? <p>{t('issueSource.noNative')}</p> : null}
            {retained.page.next_cursor ? (
              <button type="button" onClick={() => discover(true)}>
                {t('issueSource.nextNative')}
              </button>
            ) : null}
          </>
        ) : null}
        <p>
          {t('issueSource.currentOwnership')}:{' '}
          {baseline.field_policy.source_owned.join(', ') || t('common.none')}
        </p>
        <button
          type="button"
          ref={triggerRef}
          disabled={!choice || !selected}
          onClick={() => {
            if (choice && selected)
              setRequested({
                source: choice.source,
                item: selected,
                expected_field_policy: baseline.field_policy,
                scan_id: retained?.page.scan_id ?? null,
                continuation: retained?.continuation ?? null,
              });
          }}
        >
          {t('issueSource.reviewAttach')}
        </button>
      </fieldset>
      {requested ? (
        <ConfirmedCommand
          title={t('issueSource.reviewAttach')}
          confirmLabel={t('issueSource.attach')}
          consequence={t('issueSource.attachConsequence', {
            task: issue.display_key,
            native: requested.item.item,
          })}
          request={requested}
          version={baseline.issue_version}
          ready={fresh}
          execute={({ request, headers }) =>
            api.attach(
              issue.project_id,
              issue.id,
              structuredClone(request) as Attachment,
              requireVersionedMutationHeaders(headers),
            )
          }
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issueRoot(issue.project_id, issue.id) }]}
          onClose={() => setRequested(null)}
          onRefresh={onReload}
          returnFocus={
            fresh ? triggerRef.current! : (fallbackFocusRef.current ?? triggerRef.current!)
          }
          fallbackFocusRef={fallbackFocusRef}
          renderFailure={(error) =>
            error instanceof IssueSourceAssociationConflict && error.canonical ? (
              <Link
                to={`/projects/${error.canonical.selection.project_id}/issues/${error.canonical.issue_id}`}
              >
                {t('issueSource.canonical')}
              </Link>
            ) : null
          }
          renderSuccess={(result) => (
            <p>{t('issueSource.attached', { native: result.link.selection.item.item })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
