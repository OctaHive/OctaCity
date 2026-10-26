import { useState } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { SelectMenu } from '../../shared/SelectMenu';
import type { FactoryConfigurationDetails } from './api';
import type { FactoryWorkSources } from './FactoryWorkForm';
import type {
  FactorySourcesApi,
  SourceConnection,
  SourceIntent,
  SourceSnapshot,
} from './sourceApi';
import { SourcePublication } from './SourcePublication';
import { SourceReporting } from '../../shared/SourceReporting';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { sourceFailure, sourceRequest } from '../../shared/sourceErrors';
import {
  sourceReferenceKey as referenceKey,
  sourceBindingKey as bindingKey,
} from '../../shared/sourceReferences';

export type SourceRepositories = Pick<FactoryWorkSources, 'listRepositories' | 'getRepository'>;
type Input = components['schemas']['WorkSourceInput'];
const lines = (value: string) => value.split('\n').filter((label) => label.length > 0);
/** Task container authority and code Repository are selected independently. Drafts are not cached. */
export function SourceInputEditor({
  configuration,
  api,
  connections,
  repositories,
  input,
  onRefresh,
}: {
  configuration: FactoryConfigurationDetails;
  api: FactorySourcesApi;
  connections: SourceConnection[];
  repositories: SourceRepositories;
  input?: Input | undefined;
  onRefresh: () => Promise<SourceSnapshot>;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [base] = useState(input?.definition);
  const project = configuration.configuration.reference.project_id;
  const target = base?.target ?? configuration.configuration.reference;
  const [id] = useState(() => input?.id ?? crypto.randomUUID());
  const [version, setVersion] = useState(input?.version ?? null);
  const [connectionId, setConnectionId] = useState(base?.connection?.id ?? '');
  const connection = connections.find((item) => item.id === connectionId);
  const choices = useQuery({
    queryKey: queryKeys.factorySourceChoices(project, connectionId, connection?.version ?? 0),
    queryFn: ({ signal }) => api.choices(project, connectionId, connection!.version, signal),
    enabled: !!connection,
  });
  const reads = choices.data?.items.filter((item) => item.purpose === 'task_read') ?? [];
  const [resource, setResource] = useState(base?.source.resource.identity ?? '');
  const [profile, setProfile] = useState(() => (base ? bindingKey(base.source) : ''));
  const selected = reads.find(
    (item) => item.binding.resource.identity === resource && bindingKey(item.binding) === profile,
  );
  const containers = [
    ...new Map(reads.map((item) => [item.binding.resource.identity, item])).values(),
  ];
  const code = useInfiniteQuery({
    queryKey: queryKeys.factoryWorkSources(project, 'repositories'),
    queryFn: ({ pageParam, signal }) =>
      sourceRequest(repositories.listRepositories(project, pageParam, signal)),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const [repositoryId, setRepositoryId] = useState(base?.repository.id ?? '');
  const repositoryItems = code.data?.pages.flatMap((page) => page.items) ?? [];
  const repositoryVersion =
    repositoryId === base?.repository.id
      ? base!.repository.version
      : repositoryItems.find((item) => item.id === repositoryId)?.version;
  const repository = useQuery({
    queryKey: queryKeys.repository(repositoryId, repositoryVersion ?? 0),
    queryFn: ({ signal }) =>
      sourceRequest(repositories.getRepository(repositoryId, repositoryVersion!, signal)),
    enabled: !!repositoryId && !!repositoryVersion,
  });
  const [custom, setCustom] = useState(!!base?.predicate);
  const [state, setState] = useState(base?.predicate?.state ?? '');
  const [all, setAll] = useState(base?.predicate?.all_of.join('\n') ?? '');
  const [any, setAny] = useState(base?.predicate?.any_of.join('\n') ?? '');
  const [none, setNone] = useState(base?.predicate?.none_of.join('\n') ?? '');
  const [enabled, setEnabled] = useState(base?.enabled ?? false);
  const [reporterProfile, setReporterProfile] = useState(() =>
    base?.reporter ? bindingKey(base.reporter) : '',
  );
  const [reportingDraft, setReportingDraft] = useState(base?.reporting ?? null);
  const writers =
    choices.data?.items.filter(
      (item) =>
        item.purpose === 'task_update' &&
        selected &&
        referenceKey(item.binding.installation) === referenceKey(selected.binding.installation) &&
        referenceKey(item.binding.resource) === referenceKey(selected.binding.resource),
    ) ?? [];
  const selectedWriter = writers.find((item) => bindingKey(item.binding) === reporterProfile);
  // A retained Reporter must not follow a different connection or native resource.
  const reporter =
    selectedWriter?.binding ??
    (base?.reporter &&
    reporterProfile === bindingKey(base.reporter) &&
    selected &&
    connectionId === base.connection?.id &&
    referenceKey(selected.binding.installation) === referenceKey(base.reporter.installation) &&
    referenceKey(selected.binding.resource) === referenceKey(base.reporter.resource)
      ? base.reporter
      : null);
  const reporting = reporter
    ? (reportingDraft ?? selectedWriter?.reporting?.mapping ?? undefined)
    : undefined;
  const reporterOptions = writers.map((item) => ({
    value: bindingKey(item.binding),
    label: `${item.binding.configuration.identity} · ${item.binding.configuration.version}`,
  }));
  // Retaining an existing pin is distinct from offering a currently usable writer.
  if (reporter && !selectedWriter)
    reporterOptions.push({
      value: bindingKey(reporter),
      label: t('factory.retainedReporterUnavailable', { name: reporter.configuration.identity }),
    });
  const intent: SourceIntent | null =
    connection &&
    selected &&
    choices.data &&
    !choices.isError &&
    (!custom || !!selected.predicate_states) &&
    !repository.isError &&
    repository.data?.definition_digest
      ? {
          project_id: project,
          group: null,
          inputs: [
            {
              id,
              expected_version: version,
              definition: {
                target,
                repository: {
                  id: repositoryId,
                  version: repositoryVersion!,
                  project_id: project,
                  definition_digest: repository.data.definition_digest,
                },
                source: selected.binding,
                reporter,
                ...(reporter && reportingDraft ? { reporting: reportingDraft } : {}),
                enabled,
                connection: {
                  id: connection.id,
                  version: connection.version,
                  access_version: choices.data.access_version,
                  source_configuration: selected.connection_configuration,
                  reporter_configuration: reporter
                    ? (selectedWriter?.connection_configuration ??
                      base?.connection?.reporter_configuration ??
                      null)
                    : null,
                },
                ...(custom && selected.predicate_states
                  ? {
                      predicate: {
                        state: state || null,
                        all_of: lines(all),
                        any_of: lines(any),
                        none_of: lines(none),
                      },
                    }
                  : {}),
              },
            },
          ],
        }
      : null;
  const select = (
    label: string,
    value: string,
    options: { value: string; label: string }[],
    change: (value: string) => void,
    emptyLabel = t('factory.chooseInput'),
  ) => (
    <div>
      <span>{label}</span>
      <SelectMenu
        ariaLabel={label}
        value={value}
        options={[{ value: '', label: emptyLabel }, ...options]}
        onValueChange={change}
      />
    </div>
  );
  async function reconcile() {
    const { records, connections: currentConnections } = await onRefresh();
    const current = records.find(
      (record) => record.kind === 'input' && record.definition.id === id,
    );
    if (version !== null && (!current || current.kind !== 'input')) {
      throw new Error('Current input is unavailable');
    }
    const metadata = currentConnections.find((item) => item.id === connectionId);
    if (!metadata) throw new Error('Connection is unavailable');
    const currentChoices = await api.choices(project, connectionId, metadata.version);
    client.setQueryData(
      queryKeys.factorySourceChoices(project, connectionId, metadata.version),
      currentChoices,
    );
    if (current?.kind === 'input') setVersion(current.definition.version);
  }
  const selection = custom
    ? {
        state: state || t('factory.anyState'),
        all_of: lines(all),
        any_of: lines(any),
        none_of: lines(none),
      }
    : selected?.selection;
  const unrestricted =
    selection && !selection.all_of.length && !selection.any_of.length && !selection.none_of.length;
  const summary = [
    `${t('factory.sourceConnection')}: ${connection?.definition.name ?? ''} · v${connection?.version} · ${t('factory.accessVersion', { version: choices.data?.access_version ?? 0 })}`,
    `${t('factory.taskContainer')}: ${selected?.locator ?? ''}`,
    `${t('factory.inputProfile')}: ${selected?.binding.configuration.identity ?? ''} · ${selected?.binding.configuration.version ?? ''}`,
    `${t('factory.codeRepository')}: ${repository.data?.name ?? ''} · v${repositoryVersion ?? ''}`,
    `${t('factory.sourceTarget')}: ${target.id} · v${target.version}`,
    t('factory.preparationHelp'),
    t(enabled ? 'factory.inputEnabled' : 'factory.inputDisabled'),
    `${t('factory.diag.reporting')}: ${reporter ? `${reporter.configuration.identity} · ${reporter.configuration.version}` : t('common.none')}`,
    ...(reporter ? [t('factory.preserveLabels'), t('factory.reporterReadiness')] : []),
    ...(reporting?.statuses.map(
      (effect) =>
        `${effect.status}: ${t('factory.addEffect')} ${effect.add.join(', ')}; ${t('factory.removeEffect')} ${effect.remove.join(', ')}; ${t('factory.taskState')} ${effect.state ?? t('factory.preserveState')}`,
    ) ?? []),
    `${t('factory.taskState')}: ${selection?.state ?? t('factory.installedFilters')}`,
    `${t('factory.allLabels')}: ${selection?.all_of.join(' / ') ?? ''}`,
    `${t('factory.anyLabels')}: ${selection?.any_of.join(' / ') ?? ''}`,
    `${t('factory.excludedLabels')}: ${selection?.none_of.join(' / ') ?? ''}`,
  ].join('; ');
  return (
    <fieldset>
      <legend>{t(input ? 'factory.editInput' : 'factory.addInput')}</legend>
      <p>{t('factory.sourceDraftHelp')}</p>
      <p>
        {t('factory.sourceTarget')}: {target.id} · v{target.version}
      </p>
      {base?.reporter && selected && !reporter ? <p>{t('factory.reporterRemoved')}</p> : null}
      {select(
        t('factory.sourceConnection'),
        connectionId,
        connections.map((item) => ({ value: item.id, label: item.definition.name })),
        (value) => {
          setConnectionId(value);
          setReporterProfile('');
          setReportingDraft(null);
          setResource('');
          setProfile('');
          setCustom(false);
        },
      )}
      <Link to={connection ? `/integrations/${connection.id}` : '/integrations'}>
        {t('factory.manageSourceAccess')}
      </Link>
      {choices.isFetching ? (
        <p role="status">{t('query.loading', { label: t('factory.taskContainer') })}</p>
      ) : null}
      {choices.isError || code.isError || repository.isError ? (
        <QueryFailureNotice
          title={t('factory.addInput')}
          error={sourceFailure(choices.error ?? code.error ?? repository.error, t)}
          onRetry={() =>
            Promise.all(
              [choices, code, repository]
                .filter((query) => query.isError)
                .map((query) => query.refetch()),
            )
          }
        />
      ) : null}
      {connection && choices.isSuccess && !reads.length ? (
        <p>{t('factory.noSourceAccess')}</p>
      ) : null}
      {select(
        t('factory.taskContainer'),
        resource,
        containers.map((item) => ({ value: item.binding.resource.identity, label: item.locator })),
        (value) => {
          setResource(value);
          setReporterProfile('');
          setReportingDraft(null);
          setProfile('');
          setCustom(false);
        },
      )}
      {select(
        t('factory.inputProfile'),
        profile,
        reads
          .filter((item) => item.binding.resource.identity === resource)
          .map((item) => ({
            value: bindingKey(item.binding),
            label: `${item.binding.configuration.identity} · ${item.binding.configuration.version}`,
          })),
        (value) => {
          setProfile(value);
          setCustom(false);
        },
      )}
      {selected ? <p>{t('factory.preparationHelp')}</p> : null}
      <p>{t('factory.repositoryIndependent')}</p>
      {select(
        t('factory.codeRepository'),
        repositoryId,
        repositoryItems.map((item) => ({
          value: item.id,
          label: `${item.name} · v${item.id === base?.repository.id ? base!.repository.version : item.version}`,
        })),
        setRepositoryId,
      )}
      {code.hasNextPage ? (
        <button
          type="button"
          disabled={code.isFetchingNextPage}
          onClick={() => void code.fetchNextPage()}
        >
          {t('factory.moreRepositories')}
        </button>
      ) : null}
      {selected?.predicate_states ? (
        <label>
          <input
            type="checkbox"
            checked={custom}
            onChange={(event) => setCustom(event.target.checked)}
          />
          {t('factory.customizeFilters')}
        </label>
      ) : (
        <p>{t('factory.installedFilters')}</p>
      )}
      {custom && selected?.predicate_states ? (
        <>
          {select(
            t('factory.taskState'),
            state,
            selected.predicate_states.map((value) => ({ value, label: value })),
            setState,
            t('factory.anyState'),
          )}
          <label>
            {t('factory.allLabels')}
            <textarea value={all} onChange={(event) => setAll(event.target.value)} />
          </label>
          <label>
            {t('factory.anyLabels')}
            <textarea value={any} onChange={(event) => setAny(event.target.value)} />
          </label>
          <label>
            {t('factory.excludedLabels')}
            <textarea value={none} onChange={(event) => setNone(event.target.value)} />
          </label>
          <p>{t('factory.literalLabels')}</p>
        </>
      ) : selection ? (
        <p>
          {selection.state} · {selection.all_of.join(', ')} · {selection.any_of.join(', ')} ·{' '}
          {selection.none_of.join(', ')}
        </p>
      ) : null}
      {custom && selected && !selected.predicate_states ? (
        <p role="alert">{t('factory.savedFiltersUnavailable')}</p>
      ) : null}
      {unrestricted ? <p>{t('factory.unrestrictedLabels')}</p> : null}
      {repository.data && !repository.data.definition_digest ? (
        <p role="alert">{t('factory.repositoryDigestMissing')}</p>
      ) : null}
      {select(
        t('factory.reporterMapping'),
        reporterProfile,
        reporterOptions,
        (value) => {
          setReporterProfile(value);
          setReportingDraft(null);
        },
        t('factory.noReporter'),
      )}
      {!writers.length ? <p>{t('factory.noReporterAccess')}</p> : null}
      {reporter ? (
        <SourceReporting
          mapping={reporting}
          policy={selectedWriter?.reporting?.editable}
          draft={reportingDraft}
          onChange={setReportingDraft}
        />
      ) : null}
      <label>
        <input
          type="checkbox"
          checked={enabled}
          onChange={(event) => setEnabled(event.target.checked)}
        />
        {t('factory.inputEnabled')}
      </label>
      <SourcePublication
        intent={intent}
        summary={summary}
        api={api}
        onRefresh={reconcile}
        onPublished={(result) => {
          const current = result.publication.inputs.find((i) => i.input.id === id);
          if (current) setVersion(current.input.version);
        }}
      />
    </fieldset>
  );
}
