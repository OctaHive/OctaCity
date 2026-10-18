import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { SelectMenu } from '../../shared/SelectMenu';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
import { safeIntegrationError, type CredentialIntent, type IntegrationsApi } from './api';

/** The uncontrolled secret field never enters React state or the confirmed replay snapshot. */
export function CredentialSetup({
  api,
  connection,
  state,
  onClose,
  onRefresh,
}: {
  api: IntegrationsApi;
  connection: components['schemas']['IntegrationConnection'];
  state: components['schemas']['IntegrationAdministrationState'];
  onClose: () => void;
  onRefresh: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const secretRef = useRef<HTMLInputElement>(null);
  const references = [
    ...new Map(
      connection.definition.resources
        .flatMap((r) => r.capabilities)
        .map((c) => [JSON.stringify(c.credential), c.credential]),
    ).values(),
  ];
  const [selected, setSelected] = useState(0);
  const [kind, setKind] = useState<CredentialIntent['action']['kind']>(() =>
    isConfigured(references[0]) ? 'rotate' : 'provision',
  );
  const [activate, setActivate] = useState(false);
  const [review, setReview] = useState<{
    intent: CredentialIntent;
    returnFocus: HTMLElement;
  } | null>(null);
  const needsValue = kind === 'provision' || kind === 'rotate';
  useEffect(() => {
    const input = secretRef.current;
    return () => {
      if (input) input.value = '';
    };
  }, []);
  function clearSecret() {
    if (secretRef.current) secretRef.current.value = '';
  }
  function isConfigured(reference: CredentialIntent['action']['reference'] | undefined) {
    return (
      reference !== undefined &&
      state.credentials.some(
        (item) =>
          item.configured &&
          item.reference.provider === reference.provider &&
          item.reference.reference === reference.reference,
      )
    );
  }
  async function execute(attempt: ConfirmedMutationAttempt<CredentialIntent>) {
    const value = secretRef.current?.value || undefined;
    clearSecret();
    try {
      return await api.credential(
        structuredClone(attempt.request) as CredentialIntent,
        value,
        attempt.headers,
      );
    } catch (error) {
      throw safeIntegrationError(error, t('integrations.safeFailure'));
    }
  }
  const reference = references[selected];
  const configured = isConfigured(reference);
  return (
    <section aria-labelledby="credential-setup-heading">
      <h2 id="credential-setup-heading">{t('integrations.credentials')}</h2>
      <p>{t('integrations.secretHelp')}</p>
      <form
        autoComplete="off"
        onSubmit={(event) => {
          event.preventDefault();
          if (!reference || (needsValue && !secretRef.current?.value)) return;
          const button =
            event.currentTarget.querySelector<HTMLButtonElement>('button[type="submit"]');
          if (!button) return;
          setReview({
            intent: {
              id: connection.id,
              expected_metadata_version: connection.version,
              expected_revision: state.revision,
              action: {
                operation: 'credential',
                reference,
                kind,
                activate_connection: kind !== 'revoke' && activate,
              },
            },
            returnFocus: button,
          });
        }}
      >
        <div>
          <span>{t('integrations.credentialScope')}</span>
          <SelectMenu
            ariaLabel={t('integrations.credentialScope')}
            value={String(selected)}
            options={references.map((item, index) => ({
              value: String(index),
              label: `${item.provider} · ${item.reference}`,
            }))}
            onValueChange={(value) => {
              clearSecret();
              const next = Number(value);
              setSelected(next);
              setKind(isConfigured(references[next]) ? 'rotate' : 'provision');
            }}
          />
        </div>
        <p>{t(configured ? 'integrations.configured' : 'integrations.missing')}</p>
        <div>
          <span>{t('integrations.credentialOperation')}</span>
          <SelectMenu
            ariaLabel={t('integrations.credentialOperation')}
            value={kind}
            options={(['provision', 'rotate', 'adopt', 'revoke'] as const).map((value) => ({
              value,
              label: t(`integrations.${value}`),
            }))}
            onValueChange={(value) => {
              clearSecret();
              setKind(value);
            }}
          />
        </div>
        <label hidden={!needsValue}>
          {t('integrations.credentialValue')}
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            ref={secretRef}
            required={needsValue}
            disabled={!needsValue || review !== null}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={kind !== 'revoke' && activate}
            disabled={kind === 'revoke'}
            onChange={(event) => setActivate(event.target.checked)}
          />
          {t('integrations.activateWithCredential')}
        </label>
        <button type="submit" disabled={!reference || review !== null || state.preparation_pending}>
          {t('integrations.reviewCredential')}
        </button>
        <button
          type="button"
          onClick={() => {
            clearSecret();
            onClose();
          }}
        >
          {t('common.close')}
        </button>
      </form>
      {review ? (
        <ConfirmedCommand
          title={t('integrations.confirmCredential')}
          consequence={t('integrations.credentialConsequence', {
            operation: t(`integrations.${review.intent.action.kind}`),
            reference: review.intent.action.reference.reference,
            name: connection.definition.name,
          })}
          confirmLabel={t('integrations.confirmChange')}
          request={review.intent}
          execute={execute}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.integrationState(connection.id) },
            { queryKey: queryKeys.audit },
          ]}
          {...(state.revision === 0 ? {} : { version: state.revision })}
          onRefresh={onRefresh}
          onClose={() => {
            clearSecret();
            setReview(null);
          }}
          returnFocus={review.returnFocus}
          renderSuccess={(receipt) => <p>{t(`integrations.receipt.${receipt.status}`)}</p>}
        />
      ) : null}
    </section>
  );
}
