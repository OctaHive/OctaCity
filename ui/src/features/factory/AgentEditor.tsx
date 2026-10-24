import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { AgentFields } from './AgentFields';
import type { FactoryRolesApi } from './roleApi';

type Request = components['schemas']['PublishFactoryRoleProfilesRequest'];
export function AgentEditor({
  api,
  projectId,
  onDiscard,
  baseline,
}: {
  api: FactoryRolesApi;
  projectId: string;
  onDiscard: () => void;
  baseline?: components['schemas']['FactoryRole'];
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [id] = useState(() => baseline?.reference.id ?? crypto.randomUUID());
  const [definition, setDefinition] = useState<components['schemas']['FactoryRoleDefinition']>(() =>
    structuredClone(
      baseline?.definition ?? {
        name: '',
        icon: null,
        description: '',
        instructions: [''],
        concurrency_limit: null,
      },
    ),
  );
  const [head, setHead] = useState(baseline?.reference.version ?? null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{ request: Request; target: HTMLElement } | null>(null);
  const [published, setPublished] = useState(false);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  async function validate(target: HTMLElement) {
    setPending(true);
    setError(null);
    const captured = structuredClone(definition);
    try {
      const normalized = await api.validateRole(projectId, { id, definition: captured });
      setReview({
        target,
        request: {
          role: { id, definition: normalized.definition, expected_current_version: head },
          profiles: [],
        },
      });
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setPending(false);
    }
  }
  return (
    <section aria-label={t('factory.agentEditor')}>
      <p>{t('factory.memoryDraft')}</p>
      <button type="button" disabled={pending || review !== null} onClick={onDiscard}>
        {t('factory.discardDraft')}
      </button>
      <fieldset disabled={pending || review !== null}>
        <AgentFields
          api={api}
          projectId={projectId}
          value={definition}
          onChange={setDefinition}
          onPendingChange={setPending}
        />
        <button
          ref={reviewButtonRef}
          type="button"
          disabled={!definition.name.trim()}
          onClick={(event) => void validate(event.currentTarget)}
        >
          {t('factory.reviewAgent')}
        </button>
      </fieldset>
      {error ? (
        <QueryFailureNotice
          title={t('factory.agentEditor')}
          error={error}
          onRetry={() => reviewButtonRef.current && void validate(reviewButtonRef.current)}
        />
      ) : null}
      {review ? (
        <ConfirmedCommand
          title={t('factory.reviewAgent')}
          confirmLabel={t('factory.confirmPublish')}
          consequence={`${definition.name} · ${t('factory.publishConsequence')}`}
          request={review.request}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryRoleCatalog(projectId, 'roles') },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const result = await api.publish(
              projectId,
              structuredClone(attempt.request) as Request,
              attempt.headers,
            );
            setPublished(true);
            return result;
          }}
          onRefresh={async () => {
            try {
              const current = await api.currentRole(projectId, id);
              setHead(current.reference.version);
            } catch (failure) {
              setError(failure instanceof Error ? failure : new Error());
            }
          }}
          onClose={() => (published ? onDiscard() : setReview(null))}
          renderSuccess={() => <p>{t('factory.published')}</p>}
        />
      ) : null}
    </section>
  );
}
