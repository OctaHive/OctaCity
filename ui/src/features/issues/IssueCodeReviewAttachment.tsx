import { useRef, useState } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { ISSUE_SOURCE_CONSTRAINTS } from '../../../.generated/api/constraints';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import { sourceBindingKey } from '../../shared/sourceReferences';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type {
  IssueCodeReviewsApi,
  IssueReviewConnections,
  ReviewAttachment,
} from './codeReviewApi';

/** Only currently authorized repository-read choices become immutable attachment selections. */
export function IssueCodeReviewAttachment({
  issue,
  api,
  connections,
  ready,
  onReload,
}: {
  issue: Issue;
  api: IssueCodeReviewsApi;
  connections: IssueReviewConnections;
  ready: boolean;
  onReload: () => Promise<void>;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [baseline] = useState(issue.version);
  const [connectionId, setConnectionId] = useState('');
  const [repository, setRepository] = useState('');
  const [native, setNative] = useState('');
  const [request, setRequest] = useState<ReviewAttachment | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const list = useInfiniteQuery({
    queryKey: queryKeys.integrationList,
    gcTime: 0,
    queryFn: ({ pageParam, signal }) => connections.listConnections(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const items = list.data?.pages.flatMap((page) => page.items) ?? [];
  const connection = items.find((c) => c.id === connectionId);
  const access = useQuery({
    queryKey: queryKeys.integrationProjectAccess(connectionId, issue.project_id),
    gcTime: 0,
    enabled: !!connection,
    queryFn: ({ signal }) => connections.getProjectAccess(connectionId, issue.project_id, signal),
  });
  const version = access.data?.access?.connection_version;
  const metadata = useQuery({
    queryKey: queryKeys.integrationVersion(connectionId, version ?? 0),
    gcTime: 0,
    enabled: version !== undefined && version !== connection?.version,
    queryFn: ({ signal }) => connections.getConnectionVersion(connectionId, version!, signal),
  });
  const grantedConnection =
    version === undefined
      ? undefined
      : version === connection?.version
        ? connection
        : metadata.data;
  const resources = useQuery({
    queryKey: queryKeys.issueReviewResources(
      connectionId,
      issue.project_id,
      version,
      access.data?.access?.version,
    ),
    gcTime: 0,
    enabled: !!grantedConnection,
    queryFn: ({ signal }) =>
      connections.listProjectResources(connectionId, issue.project_id, signal),
  });
  const choices =
    resources.data?.resources.flatMap((resource) =>
      resource.capabilities
        .filter((capability) => capability.purpose === 'repository_read')
        .map((capability) => ({
          selection: {
            source: {
              installation: grantedConnection!.definition.installation,
              capability: capability.capability,
              configuration: capability.configuration,
              resource: resource.reference,
            },
            item: { resource: resource.native_id, item: native },
          },
          label: `${resource.locator} · ${capability.configuration.identity} / ${capability.configuration.version}`,
        })),
    ) ?? [];
  const choice = choices.find((c) => sourceBindingKey(c.selection.source) === repository);
  const fresh =
    ready &&
    issue.version === baseline &&
    !list.isFetching &&
    !list.error &&
    !access.isFetching &&
    !access.error &&
    !metadata.isFetching &&
    !metadata.error &&
    !resources.isFetching &&
    !resources.error;
  const valid =
    !!native &&
    native === native.trim() &&
    !/\p{Cc}/u.test(native) &&
    new TextEncoder().encode(native).length <= ISSUE_SOURCE_CONSTRAINTS.nativeIdentityMaximumBytes;
  return (
    <section aria-label={t('issueReviews.attachment')}>
      <p>{t('issueReviews.attachHelp')}</p>
      <Link to="/integrations">{t('issueReviews.setup')}</Link>
      {list.isPending ? (
        <QueryLoadingNotice label={t('issueReviews.connection')} />
      ) : list.error ? (
        <QueryFailureNotice
          title={t('issueReviews.connection')}
          error={list.error}
          onRetry={list.refetch}
        />
      ) : null}
      {access.error || metadata.error ? (
        <QueryFailureNotice
          title={t('issueReviews.connection')}
          error={access.error ?? metadata.error}
          onRetry={() => {
            void access.refetch();
            if (metadata.error) void metadata.refetch();
          }}
        />
      ) : null}
      {resources.error ? (
        <QueryFailureNotice
          title={t('issueReviews.repository')}
          error={resources.error}
          onRetry={resources.refetch}
        />
      ) : null}
      {list.data ? (
        <QueryBackgroundNotice
          label={t('issueReviews.connection')}
          error={null}
          fetching={list.isFetching}
          onRetry={list.refetch}
        />
      ) : null}
      <fieldset disabled={!fresh}>
        <legend>{t('issueReviews.attachment')}</legend>
        <SelectMenu
          ariaLabel={t('issueReviews.connection')}
          value={connectionId}
          options={[
            { value: '', label: t('issueReviews.chooseConnection') },
            ...items.map((c) => ({
              value: c.id,
              label: `${c.definition.name} · ${c.definition.provider}`,
            })),
          ]}
          onValueChange={(id) => {
            setConnectionId(id);
            setRepository('');
          }}
        />
        {list.hasNextPage ? (
          <button type="button" onClick={() => void list.fetchNextPage()}>
            {t('issueReviews.moreConnections')}
          </button>
        ) : null}
        <SelectMenu
          ariaLabel={t('issueReviews.repository')}
          value={repository}
          options={[
            { value: '', label: t('issueReviews.chooseRepository') },
            ...choices.map((c) => ({
              value: sourceBindingKey(c.selection.source),
              label: c.label,
            })),
          ]}
          onValueChange={setRepository}
        />
        {connection && resources.isPending ? (
          <QueryLoadingNotice label={t('issueReviews.repository')} />
        ) : connection && !choices.length ? (
          <p>{t('issueReviews.noRepository')}</p>
        ) : null}
        <label>
          {t('issueReviews.native')}
          <input
            value={native}
            maxLength={ISSUE_SOURCE_CONSTRAINTS.nativeIdentityMaximumBytes}
            onChange={(event) => setNative(event.target.value)}
          />
        </label>
        <button
          type="button"
          ref={triggerRef}
          disabled={!choice || !valid}
          onClick={() => choice && valid && setRequest({ selection: choice.selection })}
        >
          {t('issueReviews.reviewAttach')}
        </button>
      </fieldset>
      {issue.version !== baseline ? <p role="alert">{t('issueReviews.staleIssue')}</p> : null}
      {request ? (
        <ConfirmedCommand
          title={t('issueReviews.reviewAttach')}
          confirmLabel={t('issueReviews.attach')}
          consequence={t('issueReviews.attachConsequence', {
            task: issue.display_key,
            native: request.selection.item.item,
            repository: request.selection.item.resource,
            version: baseline,
          })}
          request={request}
          version={baseline}
          ready={
            fresh &&
            !!choice &&
            sourceBindingKey(choice.selection.source) === sourceBindingKey(request.selection.source)
          }
          execute={({ request, headers }) =>
            api.attach(
              issue.project_id,
              issue.id,
              structuredClone(request) as ReviewAttachment,
              requireVersionedMutationHeaders(headers),
            )
          }
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.issueRoot(issue.project_id, issue.id) }]}
          onClose={() => setRequest(null)}
          onRefresh={onReload}
          returnFocus={triggerRef.current!}
          renderSuccess={() => <p>{t('issueReviews.attached')}</p>}
        />
      ) : null}
    </section>
  );
}
