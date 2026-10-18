import { useRef, useState } from 'react';
import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import {
  safeIntegrationError,
  type Connection,
  type ConnectionIntent,
  type IntegrationsApi,
} from './api';
import { sameReference } from './references';
import { SelectMenu } from '../../shared/SelectMenu';
import styles from './Integrations.module.css';

/** In-memory metadata draft; verified server choices are the only endpoint/resource authority. */
export function ConnectionEditor({
  api,
  connection,
  onClose,
}: {
  api: IntegrationsApi;
  connection?: Connection;
  onClose: () => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const catalog = useInfiniteQuery({
    queryKey: queryKeys.integrationInstallations,
    queryFn: ({ pageParam, signal }) => api.listInstallations(pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const choices = catalog.data?.pages.flatMap((page) => page.items) ?? [];
  const [id] = useState(() => connection?.id ?? crypto.randomUUID());
  const [version, setVersion] = useState(connection?.version ?? null);
  const [name, setName] = useState(connection?.definition.name ?? '');
  const [index, setIndex] = useState<number | null>(null);
  const selectedIndex =
    index ??
    choices.findIndex((item) =>
      sameReference(item.installation, connection?.definition.installation),
    );
  const selected = choices[selectedIndex];
  const [endpoint, setEndpoint] = useState(connection?.definition.endpoint ?? '');
  const [authentication, setAuthentication] = useState(connection?.definition.authentication ?? '');
  const [resources, setResources] = useState<ConnectionIntent['draft']['resources']>(
    () =>
      connection?.definition.resources.map((resource) => ({
        reference: resource.reference,
        purposes: resource.capabilities.map((c) => c.purpose),
      })) ?? [],
  );
  const intent: ConnectionIntent | null = selected
    ? {
        id,
        expected_current_version: version,
        draft: { name, installation: selected.installation, endpoint, authentication, resources },
      }
    : null;
  const signature = JSON.stringify(intent);
  const latestRef = useRef(signature);
  latestRef.current = signature;
  const [proof, setProof] = useState<{
    signature: string;
    definition: components['schemas']['IntegrationConnectionDefinition'];
  } | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [refreshRequired, setRefreshRequired] = useState(false);
  const [review, setReview] = useState<{
    intent: ConnectionIntent;
    definition: components['schemas']['IntegrationConnectionDefinition'];
    returnFocus: HTMLElement;
  } | null>(null);
  const valid =
    !refreshRequired &&
    !!intent &&
    name.trim().length > 0 &&
    selected?.endpoints.includes(endpoint) &&
    selected.authentication.includes(authentication) &&
    resources.length > 0;
  async function validate() {
    if (!intent || !valid) return;
    setPending(true);
    setError(null);
    setProof(null);
    const captured = signature;
    try {
      const definition = await api.validate(intent);
      if (latestRef.current === captured) setProof({ signature: captured, definition });
    } catch (failure) {
      if (latestRef.current === captured)
        setError(safeIntegrationError(failure, t('integrations.safeFailure')));
    } finally {
      setPending(false);
    }
  }
  async function execute(attempt: ConfirmedMutationAttempt<ConnectionIntent>) {
    try {
      const result = await api.publish(
        structuredClone(attempt.request) as ConnectionIntent,
        attempt.headers,
      );
      setVersion(result.connection.version);
      setProof(null);
      return result;
    } catch (failure) {
      throw safeIntegrationError(failure, t('integrations.safeFailure'));
    }
  }
  async function refresh() {
    setProof(null);
    setError(null);
    setRefreshRequired(true);
    try {
      const current = await api.getConnection(id);
      setVersion(current.version);
      setRefreshRequired(false);
    } catch (failure) {
      setError(safeIntegrationError(failure, t('integrations.safeFailure')));
    }
  }
  function toggle(
    resource: NonNullable<typeof selected>['resources'][number],
    purpose: ConnectionIntent['draft']['resources'][number]['purposes'][number],
  ) {
    setResources((current) => {
      const previous =
        current.find((item) => sameReference(item.reference, resource.reference))?.purposes ?? [];
      const purposes = previous.includes(purpose)
        ? previous.filter((item) => item !== purpose)
        : [...previous, purpose];
      return [
        ...current.filter((item) => !sameReference(item.reference, resource.reference)),
        ...(purposes.length ? [{ reference: resource.reference, purposes }] : []),
      ];
    });
  }
  return (
    <section className={styles.card} aria-labelledby="connection-editor-heading">
      <h2 id="connection-editor-heading">
        {t(connection ? 'integrations.edit' : 'integrations.create')}
      </h2>
      <p>{t('integrations.draftHelp')}</p>
      <label>
        {t('integrations.name')}
        <input value={name} onChange={(event) => setName(event.target.value)} required />
      </label>
      {catalog.isPending ? <QueryLoadingNotice label={t('integrations.providers')} /> : null}
      {catalog.error ? (
        <QueryFailureNotice
          title={t('integrations.providers')}
          error={safeIntegrationError(catalog.error, t('integrations.safeFailure'))}
          onRetry={() => catalog.refetch()}
        />
      ) : null}
      <div className={styles.field}>
        <span>{t('integrations.provider')}</span>
        <SelectMenu
          ariaLabel={t('integrations.provider')}
          value={selectedIndex < 0 ? '' : String(selectedIndex)}
          options={[
            { value: '', label: t('integrations.chooseInstalled') },
            ...choices.map((choice, position) => ({
              value: String(position),
              label: `${choice.provider} · ${choice.installation.identity} · ${choice.installation.version}`,
            })),
          ]}
          onValueChange={(value) => {
            const position = value === '' ? -1 : Number(value);
            const next = choices[position];
            setIndex(position);
            setEndpoint(next?.endpoints[0] ?? '');
            setAuthentication(next?.authentication[0] ?? '');
            setResources([]);
            setProof(null);
          }}
        />
      </div>
      {!catalog.isPending && !catalog.error && !choices.length ? (
        <p>{t('integrations.noProviders')}</p>
      ) : null}
      {catalog.hasNextPage ? (
        <button
          type="button"
          disabled={catalog.isFetchingNextPage}
          onClick={() => void catalog.fetchNextPage()}
        >
          {t('integrations.moreProviders')}
        </button>
      ) : null}
      {selected ? (
        <>
          <div className={styles.field}>
            <span>{t('integrations.endpoint')}</span>
            <SelectMenu
              ariaLabel={t('integrations.endpoint')}
              value={endpoint}
              onValueChange={setEndpoint}
              options={selected.endpoints.map((value) => ({ value, label: value }))}
            />
          </div>
          <div className={styles.field}>
            <span>{t('integrations.authentication')}</span>
            <SelectMenu
              ariaLabel={t('integrations.authentication')}
              value={authentication}
              onValueChange={setAuthentication}
              options={selected.authentication.map((value) => ({ value, label: value }))}
            />
          </div>
          <h3>{t('integrations.resources')}</h3>
          {selected.resources.map((resource) => (
            <fieldset key={JSON.stringify(resource.reference)}>
              <legend>{resource.locator}</legend>
              <p>{t('integrations.nativeResource', { identity: resource.native_id })}</p>
              {resource.capabilities.map((capability) => (
                <label key={capability.purpose}>
                  <input
                    type="checkbox"
                    checked={resources.some(
                      (item) =>
                        sameReference(item.reference, resource.reference) &&
                        item.purposes.includes(capability.purpose),
                    )}
                    onChange={() => toggle(resource, capability.purpose)}
                  />
                  {t(`integrations.purpose.${capability.purpose}`)}
                </label>
              ))}
            </fieldset>
          ))}
        </>
      ) : null}
      {error ? (
        <QueryFailureNotice
          title={t('factory.validation')}
          error={error}
          onRetry={refreshRequired ? refresh : validate}
        />
      ) : null}
      {proof?.signature === signature ? <p role="status">{t('factory.validationPassed')}</p> : null}
      <div className={styles.actions}>
        <button
          type="button"
          disabled={!valid || pending || review !== null}
          onClick={() => void validate()}
        >
          {t('factory.validateDraft')}
        </button>
        <button
          type="button"
          disabled={!intent || proof?.signature !== signature || review !== null}
          onClick={(event) => {
            if (intent && proof)
              setReview({
                intent: structuredClone(intent),
                definition: proof.definition,
                returnFocus: event.currentTarget,
              });
          }}
        >
          {t('factory.reviewPublication')}
        </button>
        <button type="button" onClick={onClose}>
          {t('common.close')}
        </button>
      </div>
      {review ? (
        <ConfirmedCommand
          title={t('integrations.publishTitle')}
          confirmLabel={t('factory.confirmPublish')}
          consequence={t('integrations.publishConsequence', {
            name: review.intent.draft.name,
            provider: review.definition.provider,
            endpoint: review.definition.endpoint,
            authentication: review.definition.authentication,
            resources: review.definition.resources
              .map(
                (r) =>
                  `${r.locator}: ${r.capabilities.map((c) => t(`integrations.purpose.${c.purpose}`)).join(', ')}`,
              )
              .join('; '),
          })}
          request={review.intent}
          execute={execute}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.integrations }, { queryKey: queryKeys.audit }]}
          {...(review.intent.expected_current_version === null
            ? {}
            : { version: review.intent.expected_current_version })}
          onRefresh={refresh}
          returnFocus={review.returnFocus}
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <>
              <p>{t('integrations.published')}</p>
              <Link to={`/integrations/${encodeURIComponent(result.connection.id)}`}>
                {t('integrations.openConnection')}
              </Link>
            </>
          )}
        />
      ) : null}
    </section>
  );
}
