import { useLayoutEffect, useRef, useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { ConfigurationEditor, type ConfigurationSettings } from './ConfigurationEditor';
import { ConfigurationPolicyPicker } from './ConfigurationPolicyPicker';
import { useConfigurationPolicy } from './ConfigurationPolicyBinding';
import { FactoryPolicies } from './FactoryPolicies';
import { FactoryPublication, IncompleteFactoryPublication } from './FactoryPublication';
import type { FactoryApi, FactoryConfigurationDetails, FactoryFlowDetails } from './api';
import { factoryPolicyApi, type PolicyKind, type PolicyRecord, type PolicyRef } from './policyApi';

type Pins = Record<PolicyKind, PolicyRef['reference'] | null>;
type Legacy = NonNullable<ReturnType<typeof useConfigurationPolicy>['legacy']>;

/** Both immutable policies and resolved legacy records initialize the same Project draft. */
export function ExistingConfigurationDraft({
  baseline,
  publicationVersion,
  flows,
  api,
}: {
  baseline: FactoryConfigurationDetails['configuration'];
  publicationVersion: number;
  flows: readonly FactoryFlowDetails[];
  api: FactoryApi;
}) {
  const { t } = usePresentation();
  const { project_id: project, id } = baseline.reference;
  const launch = useConfigurationPolicy(project, 'launch', baseline.admission_policy);
  const access = useConfigurationPolicy(project, 'access', baseline.permission_ceiling);
  const pending = [launch, access].some(({ legacy, mapping }) => legacy && !mapping.isFetched);
  if (pending) return <QueryLoadingNotice label={t('policy.settings')} />;
  return (
    <ConfigurationDraft
      api={api}
      id={id}
      projectId={project}
      initial={{
        enabled: baseline.enabled,
        flow: baseline.flow,
        hard_budget: baseline.hard_budget,
        wip_limits: baseline.wip_limits,
      }}
      initialPins={{
        launch: launch.selected?.reference ?? null,
        access: access.selected?.reference ?? null,
      }}
      legacy={{ launch: launch.legacy, access: access.legacy }}
      legacyUnavailable={{ launch: launch.mapping.isError, access: access.mapping.isError }}
      flows={flows}
      publicationVersion={publicationVersion}
    />
  );
}

/** Start from a fresh defaults snapshot scoped to this draft. */
export function NewConfigurationDraft({
  api,
  projectId,
  id,
  initial,
  flows,
}: {
  api: FactoryApi;
  projectId: string;
  id: string;
  initial: ConfigurationSettings;
  flows: readonly FactoryFlowDetails[];
}) {
  const { t } = usePresentation();
  const defaults = useQuery({
    queryKey: queryKeys.factoryDraftDefaults(projectId, id),
    queryFn: ({ signal }) => factoryPolicyApi.defaults(projectId, signal),
    gcTime: 0,
  });
  if (defaults.isPending) return <QueryLoadingNotice label={t('policy.defaults')} />;
  if (!defaults.data)
    return (
      <QueryFailureNotice
        title={t('policy.defaults')}
        error={defaults.error}
        onRetry={defaults.refetch}
      />
    );
  return (
    <ConfigurationDraft
      api={api}
      projectId={projectId}
      id={id}
      initial={initial}
      flows={flows}
      initialPins={{
        launch: defaults.data.defaults?.launch_policy ?? null,
        access: defaults.data.defaults?.access_profile ?? null,
      }}
    />
  );
}

function ConfigurationDraft({
  api,
  projectId,
  id,
  initial,
  initialPins,
  legacy,
  legacyUnavailable,
  publicationVersion,
  flows,
}: {
  api: FactoryApi;
  projectId: string;
  id: string;
  initial: ConfigurationSettings;
  initialPins: Pins;
  legacy?: Record<PolicyKind, Legacy | null>;
  legacyUnavailable?: Record<PolicyKind, boolean>;
  publicationVersion?: number;
  flows: readonly FactoryFlowDetails[];
}) {
  const [originalPins] = useState(initialPins);
  const [pins, setPins] = useState(originalPins);
  const [settings, setSettings] = useState(() => structuredClone(initial));
  const [handoff, setHandoff] = useState<{ selection: PolicyRef | null } | null>(null);
  const [offers, setOffers] = useState<Partial<Record<PolicyRef['kind'], PolicyRecord>>>({});
  const returnFocusRef = useRef<HTMLButtonElement>(null);
  const backRef = useRef<HTMLButtonElement>(null);
  const { t } = usePresentation();
  useLayoutEffect(() => {
    if (handoff) backRef.current?.focus();
    else returnFocusRef.current?.focus();
  }, [handoff]);
  const definition =
    pins.launch && pins.access
      ? {
          ...settings,
          admission_policy: { project_policy: pins.launch },
          permission_ceiling: { project_policy: pins.access },
        }
      : null;
  const picker = (kind: PolicyRef['kind']) => {
    const reference = pins[kind];
    const original = originalPins[kind];
    return (
      <ConfigurationPolicyPicker
        project={projectId}
        kind={kind}
        selected={reference ? { kind, reference } : null}
        original={original ? { kind, reference: original } : null}
        onSelect={(selection) => setPins({ ...pins, [selection.kind]: selection.reference })}
        offered={offers[kind]}
        legacy={legacy?.[kind]}
        legacyUnavailable={legacyUnavailable?.[kind] ?? false}
        onManage={(trigger) => {
          returnFocusRef.current = trigger;
          setHandoff({ selection: reference ? { kind, reference } : null });
        }}
      />
    );
  };
  return (
    <>
      <div hidden={handoff !== null}>
        <ConfigurationEditor
          value={settings}
          onChange={setSettings}
          flows={flows}
          policies={{ launch: picker('launch'), access: picker('access') }}
        />
        {definition ? (
          <FactoryPublication
            api={api}
            {...(publicationVersion !== undefined ? { version: publicationVersion } : {})}
            subject={{
              kind: 'configuration',
              request: { id, project_id: projectId, definition },
            }}
          />
        ) : (
          <IncompleteFactoryPublication />
        )}
      </div>
      {handoff ? (
        <section>
          <button ref={backRef} type="button" onClick={() => setHandoff(null)}>
            {t('policy.backToDraft')}
          </button>
          <FactoryPolicies
            project={projectId}
            initialSelection={handoff.selection}
            onPublished={(record) =>
              setOffers((current) => ({ ...current, [record.kind]: record }))
            }
          />
        </section>
      ) : null}
    </>
  );
}
