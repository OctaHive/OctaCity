import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS } from '../../app/routes';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { redactManagementError } from '../../api/client';
import { catalogLabels } from './catalogLabels';

type Bindings = components['schemas']['FactoryRuntimeSettingsDefinition']['integrations'];
/** Existing integration readers are supplied by route composition, without feature-internal imports. */
export interface RuntimeIntegrationApi {
  listConnections(
    after: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationConnectionPage']>;
  getProjectAccess(
    connection: string,
    project: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationProjectAccessState']>;
  listProjectResources(
    connection: string,
    project: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationProjectResources']>;
}
/** Selects existing protected Project grants; this editor has no publication or credential port. */
export function RuntimeIntegrations({
  project,
  api,
  value,
  onChange,
}: {
  project: string;
  api: RuntimeIntegrationApi;
  value: Bindings;
  onChange: (value: Bindings) => void;
}) {
  const { t } = usePresentation();
  const [name, setName] = useState('');
  const [connectionId, setConnectionId] = useState('');
  const [selection, setSelection] = useState('');
  const connections = useCursorPage(queryKeys.integrationList, (cursor, signal) =>
    api.listConnections(cursor, signal),
  );
  const items = connections.data?.pages.flatMap((page) => page.items) ?? [];
  const connection = items.find((item) => item.id === connectionId);
  const access = useQuery({
    queryKey: queryKeys.integrationProjectAccess(connectionId, project),
    queryFn: ({ signal }) => api.getProjectAccess(connectionId, project, signal),
    enabled: !!connection,
  });
  const resources = useQuery({
    queryKey: queryKeys.integrationProjectResources(connectionId, project),
    queryFn: ({ signal }) => api.listProjectResources(connectionId, project, signal),
    enabled: !!connection,
  });
  const grant = access.data?.access;
  const available =
    connection &&
    grant &&
    connection.version === grant.connection_version &&
    !access.error &&
    !resources.error
      ? (resources.data?.resources.flatMap((resource) =>
          resource.capabilities.map((capability) => {
            const binding: Bindings[string] = {
              project_id: project,
              connection_id: connection.id,
              connection_version: grant.connection_version,
              access_version: grant.version,
              resource: resource.reference,
              capability: capability.capability,
            };
            return {
              binding,
              value: JSON.stringify(binding),
              label: `${resource.locator} · ${t(`integrations.purpose.${capability.purpose}`)}`,
            };
          }),
        ) ?? [])
      : [];
  const selected = available.find((item) => item.value === selection);
  const bounds = FACTORY_AUTHORING_SCHEMAS.FactoryRuntimeSettingsDefinition.properties.integrations;
  const validName =
    new RegExp(bounds.propertyNames.pattern).test(name) && !Object.hasOwn(value, name);
  return (
    <fieldset>
      <legend>{t('factory.integrations')}</legend>
      <p>
        {t('runtime.integrationHelp')}{' '}
        <Link
          to={
            connection
              ? `/integrations/${encodeURIComponent(connection.id)}`
              : CONSOLE_PATHS.integrations
          }
        >
          {t('runtime.manageAccess')}
        </Link>
      </p>
      {Object.entries(value).map(([alias, binding]) => (
        <p key={alias}>
          {alias} ·{' '}
          {items.find((item) => item.id === binding.connection_id)?.definition.name ??
            t('runtime.retainedBinding')}
          <button
            type="button"
            onClick={() => {
              const next = { ...value };
              delete next[alias];
              onChange(next);
            }}
          >
            {t('runtime.removeIntegration', { alias })}
          </button>
        </p>
      ))}
      <label>
        {t('runtime.integrationAlias')}
        <input
          value={name}
          maxLength={bounds.propertyNames.maxLength}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <PagedExplorerBranch
        query={connections}
        empty={t('integrations.empty')}
        labels={catalogLabels(t('section.integrations'))}
      >
        {() => null}
      </PagedExplorerBranch>
      <SelectMenu
        ariaLabel={t('runtime.connection')}
        value={connectionId}
        options={[
          { value: '', label: t('runtime.chooseConnection') },
          ...items.map((item) => ({ value: item.id, label: item.definition.name })),
        ]}
        onValueChange={(id) => {
          setConnectionId(id);
          setSelection('');
        }}
      />
      {connection && (access.isPending || resources.isPending) ? (
        <QueryLoadingNotice label={t('runtime.resources')} />
      ) : null}
      {connection && (access.error || resources.error) ? (
        <QueryFailureNotice
          title={t('runtime.resources')}
          error={redactManagementError(
            access.error ?? resources.error,
            t('integrations.safeFailure'),
          )}
          onRetry={() => {
            void access.refetch();
            void resources.refetch();
          }}
        />
      ) : null}
      {connection && access.isSuccess && resources.isSuccess && !available.length ? (
        <p>{t('runtime.noGrantedResources')}</p>
      ) : null}
      <SelectMenu
        ariaLabel={t('runtime.resources')}
        value={selection}
        options={[
          { value: '', label: t('runtime.chooseResource') },
          ...available,
          ...(selection && !selected
            ? [{ value: selection, label: t('runtime.unavailable') }]
            : []),
        ]}
        onValueChange={setSelection}
      />
      <button
        type="button"
        disabled={
          !validName ||
          !selected ||
          !!connections.error ||
          Object.keys(value).length >= bounds.maxProperties
        }
        onClick={() => {
          if (!validName || !selected) return;
          onChange({ ...value, [name]: selected.binding });
          setName('');
        }}
      >
        {t('runtime.addIntegration')}
      </button>
    </fieldset>
  );
}
