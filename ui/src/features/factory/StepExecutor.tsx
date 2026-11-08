import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { StageProfiles, type RoleServices } from './StageProfiles';
import { ExecutorSummary, StepTools } from './ExecutorSummary';
import { ExecutorPicker, sameExecutorReference } from './ExecutorPicker';
import { useRef } from 'react';
import type { FlowAuthoringServices } from './FlowPrimitives';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Props = {
  node: Node;
  choices: readonly Schema['FactoryConfigurationChoiceResource'][];
  services?: RoleServices | undefined;
  authoring?: FlowAuthoringServices | undefined;
  onChange: (patch: Partial<Node>, schemas?: Schema['FactoryFlowDataSchema'][]) => void;
};

/** Executor selection is explicit draft intent; installed server contracts own its exact binding. */
export function StepExecutor(props: Props) {
  const { t } = usePresentation();
  const latestNodeRef = useRef(props.node);
  latestNodeRef.current = props.node;
  if (props.node.kind !== 'reasoning') return null;
  const selection = props.node.profile_selection;
  const selected =
    selection?.policy.mode === 'manual' && selection.policy.candidate
      ? selection.candidates[selection.policy.candidate]?.profile
      : undefined;
  const candidates = Object.values(selection?.candidates ?? {});
  return (
    <>
      <fieldset>
        <legend>{t('factory.executor')}</legend>
        <p>
          {selected
            ? t('factory.selectedExecutorProfile', {
                name: selected.definition.name,
                version: selected.reference.version,
              })
            : selection && selection.policy.mode !== 'manual'
              ? t('factory.executorPolicyConfigured')
              : props.node.build
                ? t('factory.existingExecutorConfigured')
                : t('factory.noExecutor')}
        </p>
        {props.services && selected ? (
          <ExecutorSummary reference={selected.reference} services={props.services} />
        ) : null}
        <p>{t('factory.executorHelp')}</p>
        {props.services ? (
          <ExecutorPicker
            node={props.node}
            services={props.services}
            authoring={props.authoring}
            onRoleChange={(role) => {
              if (selection && (!role || !sameExecutorReference(selection.role, role)))
                props.onChange({ profile_selection: null, build: null });
            }}
            onResolved={(profile, build, input) => {
              const prepare =
                input &&
                !latestNodeRef.current.input_schema &&
                !latestNodeRef.current.input_binding;
              props.onChange(
                {
                  ...(prepare
                    ? {
                        input_schema: input.reference,
                        input_binding: {
                          mapping: { value: { mapping: 'object', fields: {} } },
                          sources: {},
                        },
                      }
                    : {}),
                  build: null,
                  profile_selection: {
                    role: profile.role,
                    candidates: { [profile.reference.id]: { profile, build } },
                    policy: { mode: 'manual', candidate: profile.reference.id },
                    failure: { kind: 'manual_selection' },
                    selection_budget: {
                      attempts: 0,
                      elapsed_millis: 0,
                      tokens: 0,
                      cost_micro_units: 0,
                      output_bytes: 0,
                    },
                  },
                },
                prepare ? [input] : [],
              );
            }}
          />
        ) : (
          <p>{t('factory.executorCatalogUnavailable')}</p>
        )}
        {selection || props.node.build ? (
          <details>
            <summary>{t('factory.executorSelectionPolicy')}</summary>
            <StageProfiles {...props} />
          </details>
        ) : null}
      </fieldset>
      {props.services ? (
        <StepTools
          candidates={candidates}
          permissions={props.node.permissions}
          services={props.services}
        />
      ) : null}
    </>
  );
}
