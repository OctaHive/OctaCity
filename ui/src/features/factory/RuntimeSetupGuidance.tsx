import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';

/** Installation observations guide setup; they never assert worker or MCP readiness. */
export function RuntimeSetupGuidance({
  choice,
}: {
  choice: components['schemas']['FactoryRuntimeChoice'];
}) {
  const { t } = usePresentation();
  return (
    <details>
      <summary>{t('runtime.installationPrerequisites')}</summary>
      <p>{t('runtime.installationHelp')}</p>
      <code>tools/factory-stand --factory-profiles … --factory-agent-profile …</code>
      <p>{t('runtime.installationUpdate')}</p>
      <p>{t('runtime.pluginQualification')}</p>
      {!choice.permissions.plugins.length ? <p>{t('runtime.noPluginPermission')}</p> : null}
      {!choice.permissions.network_hosts.length ? <p>{t('runtime.noNetworkPermission')}</p> : null}
      {choice.support.mcp_transports.includes('stdio') &&
      !Object.keys(choice.support.mcp_executables).length ? (
        <p>{t('runtime.noStdioExecutable')}</p>
      ) : null}
    </details>
  );
}
