import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useRef, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';

import { queryInvalidations, queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { buildPath } from '../../app/routes';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { formatEnumLabel } from '../../shared/display';
import commandFormStyles from '../../shared/OperatorCommandForm.module.css';
import { SelectMenu } from '../../shared/SelectMenu';
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
  const { t } = usePresentation();
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
      t,
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
        {t('manualBuild.start')}
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
      <div className={commandFormStyles.field}>
        <span>{t('manualBuild.source')}</span>
        <SelectMenu
          ariaLabel={t('manualBuild.source')}
          onValueChange={setSourceKind}
          options={[
            { label: t('manualBuild.defaultReference'), value: 'default_reference' },
            { label: t('manualBuild.reference'), value: 'reference' },
            { label: t('manualBuild.exactRevision'), value: 'exact_revision' },
          ]}
          value={sourceKind}
        />
      </div>
      {sourceKind === 'default_reference' ? null : (
        <label className={commandFormStyles.field}>
          <span>
            {sourceKind === 'reference'
              ? t('manualBuild.reference')
              : t('manualBuild.exactRevision')}
          </span>
          <input
            className={commandFormStyles.control}
            onChange={(event) => setSourceValue(event.target.value)}
            value={sourceValue}
          />
        </label>
      )}
      <label className={commandFormStyles.field}>
        <span>{t('manualBuild.priority')}</span>
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
            <QueryLoadingNotice label={t('manualBuild.triggers')} />
          ) : triggers.data === undefined ? (
            <QueryFailureNotice
              error={triggers.error}
              onRetry={triggers.refetch}
              title={t('manualBuild.triggerLoadFailure')}
            />
          ) : (
            <>
              <QueryBackgroundNotice
                error={triggers.error}
                fetching={triggers.isFetching && !triggers.isFetchingNextPage}
                label={t('manualBuild.triggerData')}
                onRetry={triggers.isFetchNextPageError ? triggers.fetchNextPage : triggers.refetch}
              />
              <QueryEmptyNotice>
                {triggers.hasNextPage
                  ? t('manualBuild.noTriggerLoaded')
                  : t('manualBuild.noTrigger')}
              </QueryEmptyNotice>
            </>
          )}
          {triggers.data !== undefined && triggers.hasNextPage ? (
            <button
              disabled={triggers.isFetchingNextPage}
              onClick={() => void triggers.fetchNextPage()}
              type="button"
            >
              {triggers.isFetchingNextPage
                ? t('manualBuild.searching')
                : t('manualBuild.searchNext')}
            </button>
          ) : null}
        </div>
      ) : (
        <>
          <QueryBackgroundNotice
            error={triggers.error}
            fetching={triggers.isFetching && !triggers.isFetchingNextPage}
            label={t('manualBuild.triggerData')}
            onRetry={triggers.refetch}
          />
          <button className={styles.commandButton} type="submit">
            {t('manualBuild.review')}
          </button>
        </>
      )}
      {review === null ? null : (
        <ConfirmedCommand
          confirmLabel={t('manualBuild.start')}
          consequence={t('manualBuild.consequence', {
            name: configuration.name,
            version: configuration.version,
          })}
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
                  {t('manualBuild.accepted', {
                    disposition: formatEnumLabel(result.disposition, t),
                    id: result.build_id,
                  })}
                </span>
                <Link to={buildPath(result.build_id)}>
                  {t('manualBuild.open', { id: result.build_id })}
                </Link>
              </>
            ) : (
              <span>
                {t('manualBuild.suppressed', {
                  disposition: formatEnumLabel(result.disposition, t),
                })}
              </span>
            )
          }
          request={review.request}
          returnFocus={review.returnFocus}
          title={t('manualBuild.confirmTitle', { name: configuration.name })}
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
  const { t } = usePresentation();
  const label = formatParameterName(name);
  if (valueType === 'boolean') {
    return (
      <div className={commandFormStyles.field}>
        <span>{label}</span>
        <SelectMenu
          ariaLabel={label}
          onValueChange={(nextValue) => onChange(nextValue === '' ? '' : nextValue === 'true')}
          options={[
            { label: t('manualBuild.select'), value: '' },
            { label: t('manualBuild.false'), value: 'false' },
            { label: t('manualBuild.true'), value: 'true' },
          ]}
          value={String(value)}
        />
      </div>
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
  t: ReturnType<typeof usePresentation>['t'],
): { errors: string[] } | { request: ManualBuildRequest } {
  const errors: string[] = [];
  const parameters: Record<string, ParameterValue> = {};
  for (const [name, definition] of Object.entries(configuration.definition.parameters.parameters)) {
    const value = values[name];
    if (definition.required && (value === undefined || value === '')) {
      errors.push(t('manualBuild.required', { field: formatParameterName(name) }));
      continue;
    }
    if (value === undefined || value === '') continue;
    if (definition.value_type === 'integer') {
      const parsed = typeof value === 'number' ? value : Number(value);
      if (!Number.isSafeInteger(parsed)) {
        errors.push(t('manualBuild.wholeNumber', { field: formatParameterName(name) }));
      } else {
        parameters[name] = parsed;
      }
    } else {
      parameters[name] = value;
    }
  }
  const priority = Number(priorityValue);
  if (!Number.isSafeInteger(priority)) {
    errors.push(t('manualBuild.wholeNumber', { field: t('manualBuild.priority') }));
  }
  const trimmedSource = sourceValue.trim();
  if (sourceKind !== 'default_reference' && trimmedSource.length === 0) {
    errors.push(
      t('manualBuild.required', {
        field:
          sourceKind === 'reference' ? t('manualBuild.reference') : t('manualBuild.exactRevision'),
      }),
    );
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
