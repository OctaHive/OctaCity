import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useRef, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
import { buildPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { formatEnumLabel } from '../../shared/display';
import commandFormStyles from '../../shared/OperatorCommandForm.module.css';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type {
  BuildConfigurationResource,
  ManualBuildApi,
  ManualBuildRequest,
  TriggerDefinitionSummary,
} from './api';
import styles from './ProjectDefinitions.module.css';

interface TriggerPage {
  items: TriggerDefinitionSummary[];
  next_cursor: string | null;
}

type ParameterValue = string | number | boolean;

/** Collects one typed manual invocation for the selected immutable Build Configuration. */
export function ManualBuildForm({
  api,
  configuration,
  projectId,
}: {
  api: ManualBuildApi;
  configuration: BuildConfigurationResource;
  projectId: string;
}) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const queryClient = useQueryClient();
  const triggers = useInfiniteQuery({
    getNextPageParam: (page: TriggerPage) => page.next_cursor ?? undefined,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => api.listTriggers(projectId, pageParam, signal),
    queryKey: queryKeys.projectTriggers(projectId),
  });
  const manualTrigger = triggers.data?.pages
    .flatMap((page: TriggerPage) => page.items)
    .find(
      (item) =>
        item.kind === 'manual' &&
        item.enabled &&
        item.configuration_id === configuration.id &&
        item.configuration_version === configuration.version,
    );
  const initialValues = useMemo(() => initialParameterValues(configuration), [configuration]);
  const [values, setValues] = useState<Record<string, ParameterValue>>(initialValues);
  const [sourceKind, setSourceKind] =
    useState<ManualBuildRequest['source']['kind']>('default_reference');
  const [sourceValue, setSourceValue] = useState('');
  const [priority, setPriority] = useState('0');
  const [validation, setValidation] = useState<string[]>([]);
  const [review, setReview] = useState<{
    request: ManualBuildRequest;
    returnFocus: HTMLElement;
  } | null>(null);

  const prepare = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (manualTrigger === undefined) return;
    const parsed = parseManualBuildRequest(
      configuration,
      manualTrigger,
      values,
      sourceKind,
      sourceValue,
      priority,
    );
    if ('errors' in parsed) {
      setValidation(parsed.errors);
      return;
    }
    setValidation([]);
    const submitter = (event.nativeEvent as SubmitEvent).submitter;
    if (submitter instanceof HTMLElement) {
      setReview({ request: parsed.request, returnFocus: submitter });
    }
  };

  return (
    <form className={styles.manualBuild} onSubmit={prepare}>
      <h4 ref={headingRef} tabIndex={-1}>
        Start a Build
      </h4>
      {Object.entries(configuration.definition.parameters.parameters).map(([name, parameter]) => (
        <ParameterField
          key={name}
          name={name}
          onChange={(value) => setValues((current) => ({ ...current, [name]: value }))}
          value={values[name] ?? ''}
          valueType={parameter.value_type}
        />
      ))}
      <label className={commandFormStyles.field}>
        <span>Source</span>
        <select
          className={commandFormStyles.control}
          onChange={(event) =>
            setSourceKind(event.target.value as ManualBuildRequest['source']['kind'])
          }
          value={sourceKind}
        >
          <option value="default_reference">Default reference</option>
          <option value="reference">Reference</option>
          <option value="exact_revision">Exact revision</option>
        </select>
      </label>
      {sourceKind === 'default_reference' ? null : (
        <label className={commandFormStyles.field}>
          <span>{sourceKind === 'reference' ? 'Reference' : 'Exact revision'}</span>
          <input
            className={commandFormStyles.control}
            onChange={(event) => setSourceValue(event.target.value)}
            value={sourceValue}
          />
        </label>
      )}
      <label className={commandFormStyles.field}>
        <span>Priority</span>
        <input
          className={commandFormStyles.control}
          inputMode="numeric"
          onChange={(event) => setPriority(event.target.value)}
          type="number"
          value={priority}
        />
      </label>
      {validation.length === 0 ? null : (
        <ul className={commandFormStyles.validation} role="alert">
          {validation.map((message) => (
            <li key={message}>{message}</li>
          ))}
        </ul>
      )}
      {manualTrigger === undefined ? (
        <div className={styles.commandUnavailable}>
          {triggers.isPending ? (
            <QueryLoadingNotice label="manual Triggers" />
          ) : triggers.data === undefined ? (
            <QueryFailureNotice
              error={triggers.error}
              onRetry={triggers.refetch}
              title="Manual Triggers could not be loaded."
            />
          ) : (
            <>
              <QueryBackgroundNotice
                error={triggers.error}
                fetching={triggers.isFetching && !triggers.isFetchingNextPage}
                label="manual Trigger data"
                onRetry={triggers.isFetchNextPageError ? triggers.fetchNextPage : triggers.refetch}
              />
              <QueryEmptyNotice>
                {triggers.hasNextPage
                  ? 'No enabled manual Trigger is present in the loaded pages.'
                  : 'No enabled manual Trigger is available for this configuration.'}
              </QueryEmptyNotice>
            </>
          )}
          {triggers.data !== undefined && triggers.hasNextPage ? (
            <button
              disabled={triggers.isFetchingNextPage}
              onClick={() => void triggers.fetchNextPage()}
              type="button"
            >
              {triggers.isFetchingNextPage ? 'Searching…' : 'Search next Trigger page'}
            </button>
          ) : null}
        </div>
      ) : (
        <>
          <QueryBackgroundNotice
            error={triggers.error}
            fetching={triggers.isFetching && !triggers.isFetchingNextPage}
            label="manual Trigger data"
            onRetry={triggers.refetch}
          />
          <button className={styles.commandButton} type="submit">
            Review Build
          </button>
        </>
      )}
      {review === null ? null : (
        <ConfirmedCommand
          confirmLabel="Start Build"
          consequence={`This starts one Build from configuration ${configuration.name} version ${configuration.version}.`}
          execute={({ headers, request }) => api.triggerBuild(request, headers)}
          fallbackFocusRef={headingRef}
          invalidations={[
            { queryKey: queryKeys.projectBuildPages(projectId) },
            queryInvalidations.audit,
          ]}
          onClose={() => setReview(null)}
          queryClient={queryClient}
          renderSuccess={(result) =>
            result.outcome === 'accepted' ? (
              <>
                <span>
                  Build {result.build_id} accepted ({formatEnumLabel(result.disposition)}).
                </span>
                <Link to={buildPath(result.build_id)}>Open Build {result.build_id}</Link>
              </>
            ) : (
              <span>Build suppressed ({formatEnumLabel(result.disposition)}).</span>
            )
          }
          request={review.request}
          returnFocus={review.returnFocus}
          title={`Start Build from ${configuration.name}?`}
        />
      )}
    </form>
  );
}

function ParameterField({
  name,
  onChange,
  value,
  valueType,
}: {
  name: string;
  onChange: (value: ParameterValue) => void;
  value: ParameterValue;
  valueType: 'boolean' | 'integer' | 'string';
}) {
  const label = formatParameterName(name);
  if (valueType === 'boolean') {
    return (
      <label className={commandFormStyles.field}>
        <span>{label}</span>
        <select
          className={commandFormStyles.control}
          onChange={(event) =>
            onChange(event.target.value === '' ? '' : event.target.value === 'true')
          }
          value={String(value)}
        >
          <option value="">Select…</option>
          <option value="false">False</option>
          <option value="true">True</option>
        </select>
      </label>
    );
  }
  return (
    <label className={commandFormStyles.field}>
      <span>{label}</span>
      <input
        className={commandFormStyles.control}
        inputMode={valueType === 'integer' ? 'numeric' : undefined}
        onChange={(event) => onChange(event.target.value)}
        type={valueType === 'integer' ? 'number' : 'text'}
        value={String(value)}
      />
    </label>
  );
}

function initialParameterValues(configuration: BuildConfigurationResource) {
  return Object.fromEntries(
    Object.entries(configuration.definition.parameters.parameters).map(([name, parameter]) => [
      name,
      parameter.default ?? '',
    ]),
  ) as Record<string, ParameterValue>;
}

function parseManualBuildRequest(
  configuration: BuildConfigurationResource,
  trigger: TriggerDefinitionSummary,
  values: Record<string, ParameterValue>,
  sourceKind: ManualBuildRequest['source']['kind'],
  sourceValue: string,
  priorityValue: string,
): { errors: string[] } | { request: ManualBuildRequest } {
  const errors: string[] = [];
  const parameters: Record<string, ParameterValue> = {};
  for (const [name, definition] of Object.entries(configuration.definition.parameters.parameters)) {
    const value = values[name];
    if (definition.required && (value === undefined || value === '')) {
      errors.push(`${formatParameterName(name)} is required.`);
      continue;
    }
    if (value === undefined || value === '') continue;
    if (definition.value_type === 'integer') {
      const parsed = typeof value === 'number' ? value : Number(value);
      if (!Number.isSafeInteger(parsed)) {
        errors.push(`${formatParameterName(name)} must be a whole number.`);
      } else {
        parameters[name] = parsed;
      }
    } else {
      parameters[name] = value;
    }
  }
  const priority = Number(priorityValue);
  if (!Number.isSafeInteger(priority)) errors.push('Priority must be a whole number.');
  const trimmedSource = sourceValue.trim();
  if (sourceKind !== 'default_reference' && trimmedSource.length === 0) {
    errors.push(`${sourceKind === 'reference' ? 'Reference' : 'Exact revision'} is required.`);
  }
  if (errors.length > 0) return { errors };
  return {
    request: {
      configuration_id: configuration.id,
      configuration_version: configuration.version,
      parameters,
      priority,
      source:
        sourceKind === 'default_reference'
          ? { kind: sourceKind }
          : { kind: sourceKind, value: trimmedSource },
      trigger_id: trigger.id,
      trigger_version: trigger.version,
    },
  };
}

function formatParameterName(value: string) {
  const normalized = value.replaceAll('_', ' ');
  return `${normalized.charAt(0).toUpperCase()}${normalized.slice(1)}`;
}
