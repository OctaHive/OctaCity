import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { useFactoryChoices } from './useFactoryChoices';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { factoryFieldId } from './fieldFocus';
import { PermissionEditor } from './ResourceEditors';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import type { FactoryPolicyApi } from './policyApi';

type Permissions = components['schemas']['FactoryPermissions'];
/** This selection filters editing options. Validation checks the request against installed authority. */
export function AccessPolicyFields({
  project,
  api,
  value,
  onChange,
}: {
  project: string;
  api: FactoryPolicyApi;
  value: Permissions;
  onChange: (permissions: Permissions) => void;
}) {
  const { t } = usePresentation();
  const [alias, setAlias] = useState<string | null>(null);
  const choices = useFactoryChoices({ listChoices: api.choices }, project);
  const available =
    choices.data?.pages
      .flatMap((page) => page.items)
      .filter(
        (item) => item.kind === 'permission_ceiling' && item.metadata?.permission_declarations,
      ) ?? [];
  const selected =
    alias === null && available.length === 1
      ? available[0]
      : available.find((choice) => choice.alias === alias);
  const ceiling = selected?.metadata?.permission_declarations ?? emptyFactoryPermissions();
  // Installation declarations can omit enforcement categories. Qualification is server-owned.
  const enforcement = selected?.metadata?.enforcement;
  return (
    <section>
      <p>{t('policy.accessHelp')}</p>
      {choices.isPending ? <QueryLoadingNotice label={t('policy.installedCeiling')} /> : null}
      {choices.error ? (
        <QueryFailureNotice
          title={t('policy.installedCeiling')}
          error={choices.error}
          onRetry={choices.refetch}
        />
      ) : null}
      {!choices.isPending && !available.length ? (
        <p role="status">{t('policy.noInstallation')}</p>
      ) : null}
      <label>
        {t('policy.installedCeiling')}
        <SelectMenu
          ariaLabel={t('policy.installedCeiling')}
          value={selected?.alias ?? ''}
          options={[
            { value: '', label: t('policy.chooseCeiling') },
            ...available.map((item) => ({ value: item.alias, label: item.alias })),
          ]}
          onValueChange={setAlias}
        />
      </label>
      {choices.hasNextPage ? (
        <button
          type="button"
          onClick={() => void choices.fetchNextPage()}
          disabled={choices.isFetchingNextPage}
        >
          {t('policy.moreInstallation')}
        </button>
      ) : null}
      <p>{t('policy.installationHelp')}</p>
      <details>
        <summary>{t('policy.supportedAccess')}</summary>
        <fieldset id={factoryFieldId('/definition/permissions')} tabIndex={-1}>
          <legend>{t('policy.requestedAccess')}</legend>
          <PermissionEditor
            label={t('policy.permissions')}
            value={value}
            ceiling={ceiling}
            choices={ceiling}
            limitNumbers={false}
            {...(enforcement?.length ? { enforcement } : {})}
            onChange={onChange}
          />
        </fieldset>
      </details>
    </section>
  );
}
