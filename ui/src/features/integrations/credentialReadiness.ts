import type { components } from '../../../.generated/api/schema';
import type { CredentialIntent } from './api';

type State = components['schemas']['IntegrationAdministrationState'];
type Reference = CredentialIntent['action']['reference'];

function sameCredential(left: Reference, right: Reference | undefined) {
  return left.provider === right?.provider && left.reference === right?.reference;
}

/** Retained configuration is an input prerequisite, not proof of remote permissions. */
export function credentialConfigured(state: State, reference: Reference | undefined) {
  return state.credentials.some(
    (item) => item.configured && sameCredential(item.reference, reference),
  );
}

/** Advisory form gating; the server remains authoritative at confirmation. */
export function connectionCredentialsReady(
  connection: components['schemas']['IntegrationConnection'],
  state: State,
  beingConfigured?: Reference,
) {
  return connection.definition.resources.every((resource) =>
    resource.capabilities.every(
      (capability) =>
        sameCredential(capability.credential, beingConfigured) ||
        credentialConfigured(state, capability.credential),
    ),
  );
}
