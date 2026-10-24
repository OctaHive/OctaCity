import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { FactoryApi, FactoryConfigurationDetails } from './api';

type Override = components['schemas']['FactoryProfileAdmissionOverride'];
/** Walks only the retained closure; no candidate is discovered or chosen by the browser. */
function stages(flow: FactoryConfigurationDetails['configuration']['flow']) {
  const result: {
    label: string;
    stage: Override['stage'];
    candidates: NonNullable<
      components['schemas']['FactoryFlowNodeDefinition']['profile_selection']
    >['candidates'];
  }[] = [];
  function visit(reference: typeof flow.closure.root, calls: string[]) {
    if (calls.length >= flow.limits.max_depth) return;
    const definition = flow.closure.definitions.find(
      (item) =>
        item.reference.id === reference.id &&
        item.reference.version === reference.version &&
        item.reference.digest === reference.digest,
    );
    for (const node of definition?.nodes ?? []) {
      if (node.profile_selection)
        result.push({
          label: [...calls, node.key].join(' / '),
          stage: { root: flow.closure.root, calls, node: node.key },
          candidates: node.profile_selection.candidates,
        });
      if (node.subflow) visit(node.subflow, [...calls, node.key]);
    }
  }
  visit(flow.closure.root, []);
  return result;
}
export function WorkProfileOverrides({
  api,
  configuration,
  value,
  onChange,
}: {
  api: FactoryApi;
  configuration: { id: string; version: number };
  value: readonly Override[];
  onChange: (value: Override[]) => void;
}) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const details = useQuery({
    queryKey: queryKeys.factoryConfiguration(configuration.id, configuration.version),
    enabled: open,
    queryFn: ({ signal }) => api.getConfiguration(configuration.id, configuration.version, signal),
  });
  return (
    <section>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
        {t('factory.overrideProfiles')}
      </button>
      {open ? (
        details.data ? (
          <>
            <QueryBackgroundNotice
              label={t('factory.stageProfiles')}
              error={details.error}
              fetching={details.isFetching}
              onRetry={details.refetch}
            />
            {stages(details.data.configuration.flow).map(({ label, stage, candidates }) => {
              const identity = JSON.stringify(stage);
              const selected = value.find(
                (item) => JSON.stringify(item.stage) === identity,
              )?.profile;
              return (
                <SelectMenu
                  key={identity}
                  disabled={!!details.error}
                  ariaLabel={t('factory.profileOverride', { stage: label })}
                  value={
                    Object.entries(candidates).find(
                      ([, item]) =>
                        item.profile.reference.digest === selected?.digest &&
                        item.profile.reference.id === selected.id,
                    )?.[0] ?? ''
                  }
                  options={[
                    { value: '', label: t('factory.keepStageProfile') },
                    ...Object.entries(candidates).map(([alias, item]) => ({
                      value: alias,
                      label: `${alias} · ${item.profile.definition.name} v${item.profile.reference.version}`,
                    })),
                  ]}
                  onValueChange={(alias) =>
                    onChange([
                      ...value.filter((item) => JSON.stringify(item.stage) !== identity),
                      ...(candidates[alias]
                        ? [{ stage, profile: candidates[alias].profile.reference }]
                        : []),
                    ])
                  }
                />
              );
            })}
          </>
        ) : details.isPending ? (
          <QueryLoadingNotice label={t('factory.stageProfiles')} />
        ) : (
          <QueryFailureNotice
            title={t('factory.stageProfiles')}
            error={details.error}
            onRetry={details.refetch}
          />
        )
      ) : null}
    </section>
  );
}
