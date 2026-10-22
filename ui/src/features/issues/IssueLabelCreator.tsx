import { useRef, useState, type RefObject } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { IssuePropertiesPublication, IssuePropertyCatalog, IssuesApi } from './api';

/** Adds one project label by publishing the complete retained catalog through its existing contract. */
export function IssueLabelCreator({
  api,
  catalog,
  ready,
  onClose,
  onPublished,
  fallbackFocusRef,
}: {
  api: IssuesApi;
  catalog: IssuePropertyCatalog;
  ready: boolean;
  onClose: () => void;
  onPublished: (catalog: IssuePropertyCatalog) => void;
  fallbackFocusRef: RefObject<HTMLButtonElement | null>;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const [name, setName] = useState('');
  const [review, setReview] = useState<IssuePropertiesPublication | null>(null);
  const [saved, setSaved] = useState(false);
  const reviewLabel = () => {
    if (!ready || !name.trim()) return;
    setReview({
      expected_version: catalog.version,
      definition: {
        ...structuredClone(catalog.definition),
        labels: [
          ...catalog.definition.labels,
          {
            id: `label_${crypto.randomUUID().replaceAll('-', '')}`,
            name: name.trim(),
            retired: false,
          },
        ],
      },
    });
  };
  return (
    <section aria-label={t('planning.createLabel')}>
      <div>
        <label>
          {t('planning.labelName')}
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Enter' || event.nativeEvent.isComposing) return;
              event.preventDefault();
              reviewLabel();
            }}
          />
        </label>
        <button
          type="button"
          ref={reviewButtonRef}
          disabled={!ready || !name.trim()}
          onClick={reviewLabel}
        >
          {t('planning.reviewLabel')}
        </button>
        <button type="button" onClick={onClose}>
          {t('common.close')}
        </button>
      </div>
      {review && !saved && catalog.version !== review.expected_version ? (
        <p role="alert">{t('planning.labelCatalogChanged')}</p>
      ) : null}
      {review ? (
        <ConfirmedCommand
          fallbackFocusRef={fallbackFocusRef}
          ready={ready && catalog.version === review.expected_version}
          title={t('planning.reviewLabel')}
          confirmLabel={t('planning.createLabel')}
          consequence={t('planning.labelConsequence', {
            name: review.definition.labels.at(-1)!.name,
          })}
          request={review}
          returnFocus={reviewButtonRef.current!}
          queryClient={queryClient}
          invalidations={[
            { queryKey: queryKeys.issuePropertyChoices(catalog.project_id), exact: true },
          ]}
          execute={async (attempt) => {
            const result = await api.publishProperties(
              catalog.project_id,
              structuredClone(attempt.request) as IssuePropertiesPublication,
              attempt.headers,
            );
            onPublished(result.resource);
            setSaved(true);
            return result;
          }}
          onRefresh={async () => {
            await queryClient.invalidateQueries({
              queryKey: queryKeys.issuePropertyChoices(catalog.project_id),
              exact: true,
            });
            onClose();
          }}
          onClose={() => {
            setReview(null);
            if (saved) onClose();
          }}
          renderSuccess={() => <p>{t('planning.labelCreated')}</p>}
        />
      ) : null}
    </section>
  );
}
