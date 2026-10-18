import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { SelectMenu } from '../../shared/SelectMenu';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import { CredentialSetup } from './CredentialSetup';
import { ConnectionEditor } from './ConnectionEditor';
import {
  integrationsApi,
  safeIntegrationError,
  type CheckResult,
  type Connection,
  type IntegrationsApi,
  type StateIntent,
} from './api';
import styles from './Integrations.module.css';
import { ProjectAccessPanel } from './ProjectAccess';
import type { ProjectHierarchyApi } from '../../shared/projectHierarchy';

export function IntegrationDetailView({
  api = integrationsApi,
  projects,
}: {
  api?: IntegrationsApi | undefined;
  projects?: Pick<ProjectHierarchyApi, 'listProjects'> | undefined;
}) {
  const { integrationId } = useParams();
  return integrationId ? (
    <ConnectionDetails key={integrationId} id={integrationId} api={api} projects={projects} />
  ) : null;
}

/** Connection state, credential setup and read-only readiness retain separate authority. */
export function ConnectionDetails({
  api,
  id,
  projects,
}: {
  api: IntegrationsApi;
  id: string;
  projects?: Pick<ProjectHierarchyApi, 'listProjects'> | undefined;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const metadata = useQuery({
    queryKey: queryKeys.integration(id),
    queryFn: ({ signal }) => api.getConnection(id, signal),
  });
  const administration = useQuery({
    queryKey: queryKeys.integrationState(id),
    queryFn: ({ signal }) => api.getState(id, signal),
  });
  const [panel, setPanel] = useState<'edit' | 'credentials' | 'access' | null>(null);
  const [review, setReview] = useState<{ intent: StateIntent; returnFocus: HTMLElement } | null>(
    null,
  );
  if (metadata.isPending) return <QueryLoadingNotice label={t('integrations.connection')} />;
  if (!metadata.data)
    return (
      <QueryFailureNotice
        title={t('integrations.connection')}
        error={safeIntegrationError(metadata.error, t('integrations.safeFailure'))}
        onRetry={() => metadata.refetch()}
      />
    );
  const connection = metadata.data;
  const state = administration.data;
  async function execute(attempt: ConfirmedMutationAttempt<StateIntent>) {
    try {
      return await api.setState(structuredClone(attempt.request) as StateIntent, attempt.headers);
    } catch (error) {
      throw safeIntegrationError(error, t('integrations.safeFailure'));
    }
  }
  return (
    <div className={styles.view}>
      <section className={styles.card} aria-labelledby="connection-heading">
        <Link to="/integrations">{t('integrations.allConnections')}</Link>
        <h1 id="connection-heading">{connection.definition.name}</h1>
        <QueryBackgroundNotice
          label={t('integrations.connection')}
          error={
            metadata.error
              ? safeIntegrationError(metadata.error, t('integrations.safeFailure'))
              : null
          }
          fetching={metadata.isFetching}
          onRetry={() => metadata.refetch()}
        />
        <dl className={styles.details}>
          <div>
            <dt>{t('integrations.provider')}</dt>
            <dd>{connection.definition.provider}</dd>
          </div>
          <div>
            <dt>{t('integrations.endpoint')}</dt>
            <dd>{connection.definition.endpoint}</dd>
          </div>
          <div>
            <dt>{t('integrations.authentication')}</dt>
            <dd>{connection.definition.authentication}</dd>
          </div>
          <div>
            <dt>{t('integrations.metadataVersion')}</dt>
            <dd>{connection.version}</dd>
          </div>
        </dl>
        <p>{t('integrations.separateAccess')}</p>
        <div className={styles.actions}>
          {projects ? (
            <button
              type="button"
              disabled={!!metadata.error}
              onClick={() => setPanel(panel === 'access' ? null : 'access')}
            >
              {t('integrations.manageAccess')}
            </button>
          ) : null}
          <button type="button" disabled={!!metadata.error} onClick={() => setPanel('edit')}>
            {t('integrations.edit')}
          </button>
        </div>
      </section>
      <section className={styles.card} aria-labelledby="connection-state-heading">
        <h2 id="connection-state-heading">{t('integrations.administration')}</h2>
        {administration.isPending ? (
          <QueryLoadingNotice label={t('integrations.administration')} />
        ) : null}
        {administration.error ? (
          <QueryFailureNotice
            title={t('integrations.administration')}
            error={safeIntegrationError(administration.error, t('integrations.safeFailure'))}
            onRetry={() => administration.refetch()}
          />
        ) : null}
        {state ? (
          <>
            <p>{t(state.enabled ? 'integrations.enabled' : 'integrations.disabled')}</p>
            <p>
              {t('integrations.activeVersion', {
                version: state.active_version ?? t('integrations.none'),
              })}
            </p>
            {state.preparation_pending ? (
              <p role="status">{t('integrations.preparationPending')}</p>
            ) : null}
            <div className={styles.actions}>
              <button
                type="button"
                disabled={!!administration.error || state.preparation_pending}
                onClick={() => setPanel('credentials')}
              >
                {t('integrations.manageCredentials')}
              </button>
              <button
                type="button"
                disabled={!!administration.error || state.preparation_pending}
                onClick={(event) =>
                  setReview({
                    intent: {
                      id,
                      expected_metadata_version: connection.version,
                      expected_revision: state.revision,
                      action: { operation: state.enabled ? 'disable' : 'activate' },
                    },
                    returnFocus: event.currentTarget,
                  })
                }
              >
                {t(state.enabled ? 'integrations.disable' : 'integrations.activate')}
              </button>
            </div>
          </>
        ) : null}
      </section>
      {panel === 'access' && projects ? (
        <ProjectAccessPanel
          key={connection.version}
          api={api}
          connection={connection}
          projects={projects}
        />
      ) : null}
      {panel === 'edit' ? (
        <ConnectionEditor api={api} connection={connection} onClose={() => setPanel(null)} />
      ) : null}
      {panel === 'credentials' && state ? (
        <section className={styles.card}>
          <CredentialSetup
            api={api}
            connection={connection}
            state={state}
            onClose={() => setPanel(null)}
            onRefresh={() => administration.refetch()}
          />
        </section>
      ) : null}
      <ConnectionChecks key={`${id}:${connection.version}`} api={api} connection={connection} />
      {review ? (
        <ConfirmedCommand
          title={t('integrations.confirmState')}
          consequence={t(
            review.intent.action.operation === 'disable'
              ? 'integrations.disableConsequence'
              : 'integrations.activateConsequence',
          )}
          confirmLabel={t('integrations.confirmChange')}
          request={review.intent}
          execute={execute}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.integrationState(id) },
            { queryKey: queryKeys.audit },
          ]}
          {...(review.intent.expected_revision === 0
            ? {}
            : { version: review.intent.expected_revision })}
          returnFocus={review.returnFocus}
          onClose={() => setReview(null)}
          onRefresh={() => administration.refetch()}
          renderSuccess={(receipt) => <p>{t(`integrations.receipt.${receipt.status}`)}</p>}
        />
      ) : null}
    </div>
  );
}

function ConnectionChecks({ api, connection }: { api: IntegrationsApi; connection: Connection }) {
  const { t } = usePresentation();
  const options = connection.definition.resources.flatMap((resource) =>
    resource.capabilities.map((capability) => ({ resource, purpose: capability.purpose })),
  );
  const [selected, setSelected] = useState(0);
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<CheckResult | null>(null);
  const [error, setError] = useState<Error | null>(null);
  async function check() {
    const choice = options[selected];
    if (!choice) return;
    setPending(true);
    setError(null);
    setResult(null);
    try {
      setResult(
        await api.check(connection.id, {
          version: connection.version,
          resource: choice.resource.reference,
          purpose: choice.purpose,
        }),
      );
    } catch (failure) {
      setError(safeIntegrationError(failure, t('integrations.safeFailure')));
    } finally {
      setPending(false);
    }
  }
  return (
    <section className={styles.card} aria-labelledby="connection-checks-heading">
      <h2 id="connection-checks-heading">{t('integrations.readiness')}</h2>
      <p>{t('integrations.checkHelp')}</p>
      <div className={styles.field}>
        <span>{t('integrations.checkTarget')}</span>
        <SelectMenu
          ariaLabel={t('integrations.checkTarget')}
          value={String(selected)}
          disabled={pending}
          options={options.map((choice, index) => ({
            value: String(index),
            label: `${choice.resource.locator} · ${t(`integrations.purpose.${choice.purpose}`)}`,
          }))}
          onValueChange={(value) => {
            setSelected(Number(value));
            setResult(null);
            setError(null);
          }}
        />
      </div>
      <button type="button" disabled={pending || !options.length} onClick={() => void check()}>
        {t(pending ? 'integrations.checking' : 'integrations.check')}
      </button>
      {error ? (
        <QueryFailureNotice title={t('integrations.readiness')} error={error} onRetry={check} />
      ) : null}
      {result ? (
        <div role="status">
          <p>
            {t('integrations.checkedVersion', { version: result.connection_version })} ·{' '}
            {result.locator} · {t(`integrations.purpose.${result.purpose}`)}
          </p>
          <dl className={styles.details}>
            {(['configuration', 'authentication', 'resource_access', 'capability'] as const).map(
              (field) => (
                <div key={field}>
                  <dt>{t(`integrations.checkField.${field}`)}</dt>
                  <dd>{t(`integrations.readiness.${result.readiness[field]}`)}</dd>
                </div>
              ),
            )}
          </dl>
          {result.readiness.retry_after_millis === null ? null : (
            <p>
              {t('query.retryDelay', {
                seconds: Math.ceil(result.readiness.retry_after_millis / 1000),
              })}
            </p>
          )}
        </div>
      ) : null}
    </section>
  );
}
