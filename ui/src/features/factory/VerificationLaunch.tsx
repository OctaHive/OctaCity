import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';

type S = components['schemas'];
type Root = S['FactoryFlowRootDraft'];
type Node = S['FactoryFlowNodeDefinition'];
type Reference = S['FactoryImmutableReferenceResource'];

export function InstalledVerificationLaunch({
  services,
  ...props
}: {
  services: FlowAuthoringServices;
  root: Root;
  node: Node;
  onChange: (root: Root) => void;
}) {
  const { data } = useFactoryAuthoringCapabilities(services);
  return (
    <VerificationLaunch
      {...props}
      profiles={data?.scoped_validation_profiles}
      runners={data?.verification_runners}
    />
  );
}

function models(node: Node) {
  return [
    node.build,
    ...Object.values(node.profile_selection?.candidates ?? {}).map((entry) => entry.build),
  ]
    .filter((build) => build != null)
    .map((build) => sourceReferenceKey(build.model_or_tool));
}

/** Changes only who starts the existing verifier; its inputs and result stay pinned. */
export function VerificationLaunch({
  root,
  node,
  profiles = [],
  runners = [],
  onChange,
}: {
  root: Root;
  node: Node;
  profiles?: Reference[] | undefined;
  runners?: Reference[] | undefined;
  onChange: (root: Root) => void;
}) {
  const { t } = usePresentation();
  const bound = root.nodes.find((caller) => caller.scoped_validation_runner === node.key);
  if (
    node.kind !== 'build_command' ||
    (!bound &&
      !models(node).some((model) => runners.some((runner) => sourceReferenceKey(runner) === model)))
  )
    return null;
  const supported = new Set(profiles.map(sourceReferenceKey));
  const callers = root.nodes.filter((caller) => {
    const selected = models(caller);
    return (
      caller.kind === 'reasoning' &&
      selected.length > 0 &&
      selected.every((model) => supported.has(model)) &&
      (!caller.scoped_validation_runner || caller.scoped_validation_runner === node.key)
    );
  });
  return (
    <fieldset>
      <legend>{t('factory.verificationLaunch')}</legend>
      <p>{t('factory.verificationLaunchHelp')}</p>
      <label>
        {t('factory.verificationLaunch')}
        <SelectMenu
          ariaLabel={t('factory.verificationLaunch')}
          value={bound?.key ?? ''}
          options={[
            { value: '', label: t('factory.verificationDirect') },
            ...callers.map((caller) => ({ value: caller.key, label: `Tester · ${caller.key}` })),
            ...(bound && !callers.includes(bound)
              ? [{ value: bound.key, label: `Tester · ${bound.key}` }]
              : []),
          ]}
          onValueChange={(callerKey) =>
            onChange({
              ...root,
              nodes: root.nodes.map((caller) =>
                caller.key === callerKey
                  ? { ...caller, scoped_validation_runner: node.key }
                  : caller.scoped_validation_runner === node.key
                    ? { ...caller, scoped_validation_runner: null }
                    : caller,
              ),
              execution: callerKey
                ? {
                    ...root.execution,
                    max_active_nodes: Math.max(2, root.execution.max_active_nodes),
                  }
                : root.execution,
            })
          }
        />
      </label>
      {!callers.length ? <p>{t('factory.verificationNeedsTester')}</p> : null}
      {bound ? <p>{t('factory.verificationSiblingHelp')}</p> : null}
    </fieldset>
  );
}
