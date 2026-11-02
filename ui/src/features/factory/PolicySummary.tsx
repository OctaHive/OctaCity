import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { PolicyRecord } from './policyApi';
import { PermissionEditor } from './ResourceEditors';

/** Render retained rules without inferring authority from names or defaults. */
export function PolicySummary({ record }: { record: PolicyRecord }) {
  const { t } = usePresentation();
  if (record.kind === 'launch')
    return (
      <>
        <p>
          {t(
            record.policy.definition.allow_manual ? 'policy.manualAllowed' : 'policy.manualDenied',
          )}
        </p>
        <p>
          {t(
            record.policy.definition.allow_automatic_source
              ? 'policy.automaticAllowed'
              : 'policy.automaticDenied',
          )}
        </p>
        <p>{t('policy.sourceHelp')}</p>
      </>
    );
  const permissions = record.policy.definition.permissions;
  return (
    <>
      <p>{t('policy.accessHelp')}</p>
      <p>
        {t('policy.accessSummary', {
          plugins: permissions.plugins.length,
          executables: permissions.executables.length,
          tools: permissions.tools.length,
          hosts: permissions.network_hosts.length,
        })}
      </p>
      {!permissions.plugins.length &&
      !permissions.executables.length &&
      !permissions.tools.length ? (
        <p>{t('policy.noExecution')}</p>
      ) : null}
      <PolicyPermissionDetails permissions={permissions} />
    </>
  );
}

export function PolicyPermissionDetails({
  permissions,
}: {
  permissions: components['schemas']['FactoryPermissions'];
}) {
  const { t } = usePresentation();
  return (
    <details>
      <summary>{t('policy.permissions')}</summary>
      <fieldset disabled>
        <PermissionEditor
          label={t('policy.permissions')}
          value={permissions}
          choices={permissions}
          ceiling={permissions}
          onChange={() => undefined}
        />
      </fieldset>
    </details>
  );
}
