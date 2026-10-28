import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

/** Logical references and installed read scope only; this component never resolves credentials. */
export function McpAuthorizationSummary({
  authorization: auth,
}: {
  authorization: components['schemas']['FactoryMcpAuthorization'];
}) {
  const { t } = usePresentation();
  return (
    <dl>
      <dt>{t('factory.secretReference')}</dt>
      <dd>
        {auth.secret.provider} · {auth.secret.reference}
      </dd>
      <dt>{t('runtime.credentialProfile')}</dt>
      <dd>{auth.credential_profile}</dd>
      <dt>{t('runtime.accessScope')}</dt>
      <dd>{t('runtime.readOnlyAccess')}</dd>
      <dt>{t('runtime.resources')}</dt>
      <dd>
        {auth.resource.identity} · {auth.resource.version}
      </dd>
    </dl>
  );
}
