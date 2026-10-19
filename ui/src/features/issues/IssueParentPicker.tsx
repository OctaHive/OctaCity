import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { SelectMenu } from '../../shared/SelectMenu';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { Issue, IssuesApi } from './api';

/** Select only visible Project tasks; cycle and cross-Project checks stay authoritative on server. */
export function IssueParentPicker({
  api,
  issue,
  value,
  selectedLabel,
  disabled,
  onChange,
}: {
  api: IssuesApi;
  issue: Issue;
  value: string | null;
  selectedLabel: string;
  disabled: boolean;
  onChange: (id: string | null, label: string | null) => void;
}) {
  const { t } = usePresentation();
  const [after, setAfter] = useState<string | null>(null);
  const list = useQuery({
    queryKey: queryKeys.issueList(issue.project_id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.list(issue.project_id, after, signal),
  });
  const items = list.data?.items.filter((i) => i.id !== issue.id) ?? [];
  return (
    <div>
      {!list.data ? (
        list.isPending ? (
          <QueryLoadingNotice label={t('planning.parent')} />
        ) : (
          <QueryFailureNotice
            title={t('planning.parent')}
            error={list.error}
            onRetry={list.refetch}
          />
        )
      ) : null}
      <QueryBackgroundNotice
        label={t('planning.parent')}
        error={list.data ? list.error : null}
        fetching={list.isFetching && !!list.data}
        onRetry={list.refetch}
      />
      <span>{t('planning.parent')}</span>
      <SelectMenu
        ariaLabel={t('planning.parent')}
        value={value ?? ''}
        disabled={disabled || !list.data || list.isFetching || !!list.error}
        options={[
          { value: '', label: t('planning.noParent') },
          ...(value && !items.some((i) => i.id === value) ? [{ value, label: selectedLabel }] : []),
          ...items.map((i) => ({ value: i.id, label: `${i.display_key} · ${i.title}` })),
        ]}
        onValueChange={(id) => {
          if (id === value) return;
          const item = items.find((i) => i.id === id);
          onChange(item?.id ?? null, item ? `${item.display_key} · ${item.title}` : null);
        }}
      />
      {after ? (
        <button type="button" disabled={list.isFetching} onClick={() => setAfter(null)}>
          {t('planning.firstParents')}
        </button>
      ) : null}
      {list.data?.next_cursor ? (
        <button
          type="button"
          disabled={list.isFetching || !!list.error}
          onClick={() => setAfter(list.data!.next_cursor)}
        >
          {t('planning.nextParents')}
        </button>
      ) : null}
    </div>
  );
}
