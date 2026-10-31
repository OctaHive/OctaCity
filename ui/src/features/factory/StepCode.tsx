import type { FlowAuthoringServices } from './FlowPrimitives';
import { useQuery, useQueries } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { CodeScope } from './CodeScope';
import { type CodeResourceServices } from './FlowCodeSources';
type S = components['schemas'];
/** A per-step input request; documents, Work correlation and inventory alone grant no code. */
export function StepCode({
  node,
  root,
  services,
  onChange,
  authoring,
}: {
  authoring?: FlowAuthoringServices | undefined;
  node: S['FactoryFlowNodeDefinition'];
  root: S['FactoryFlowRootDraft'];
  services: CodeResourceServices;
  onChange: (patch: Partial<S['FactoryFlowNodeDefinition']>) => void;
}) {
  const { t } = usePresentation();
  const capability = useQuery({
    queryKey: queryKeys.factoryAuthoringCapabilities(authoring?.projectId ?? ''),
    enabled: !!authoring,
    queryFn: ({ signal }) => authoring!.api.getAuthoringCapabilities(authoring!.projectId, signal),
  });
  const external = ['reasoning', 'build_command', 'decision_signal'].includes(node.kind);
  const sourceFreeUnsupported =
    external &&
    node.code?.kind === 'none' &&
    capability.data?.source_free_incompatibility === 'executor_support_required';
  const gateSourceUnsupported =
    (node.kind === 'human_gate' || node.kind === 'deterministic_gate') &&
    node.code?.kind === 'source';
  const sources = Object.entries(root.code_sources ?? {});
  const code = node.code;
  const metadata = useQueries({
    queries: sources.map(([, source]) => ({
      queryKey: queryKeys.repository(source.repository.id, source.repository.version),
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        services.repositories.getRepository(
          source.repository.id,
          source.repository.version,
          signal,
        ),
    })),
  });
  return (
    <fieldset>
      <legend>{t('factory.stepCode')}</legend>
      <p>{t('factory.stepCodeHelp')}</p>
      {sourceFreeUnsupported ? <p role="alert">{t('factory.sourceFreeNotConfirmed')}</p> : null}
      {gateSourceUnsupported ? <p role="alert">{t('factory.gateCodeUnsupported')}</p> : null}
      {!code ? <p role="alert">{t('factory.legacyCodeSelection')}</p> : null}
      <SelectMenu
        ariaLabel={t('factory.stepCodeSelection')}
        value={code?.kind === 'none' ? 'none' : code?.kind === 'source' ? code.source : ''}
        options={[
          ...(!code ? [{ value: '', label: t('factory.legacyCodeSelection') }] : []),
          { value: 'none', label: t('factory.noCode') },
          ...sources.map(([alias, source], index) => ({
            value: alias,
            label: `${metadata[index]?.data?.name ?? alias} · v${source.repository.version}`,
          })),
          ...(code?.kind === 'source' && !Object.hasOwn(root.code_sources ?? {}, code.source)
            ? [{ value: code.source, label: t('factory.codeSourceMissing') }]
            : []),
        ]}
        onValueChange={(alias) => {
          if (alias === 'none') onChange({ code: { kind: 'none' } });
          else {
            const source = root.code_sources?.[alias];
            if (source)
              onChange({
                code: {
                  kind: 'source',
                  source: alias,
                  revision: { kind: 'admitted_base' },
                  access: 'read_only',
                  scope: structuredClone(source.scope),
                },
              });
          }
        }}
      />
      {code?.kind === 'source' ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.stepCodeAccess')}
            value={code.access}
            options={[
              { value: 'read_only', label: t('factory.codeRead') },
              { value: 'read_write', label: t('factory.codeWrite') },
            ]}
            onValueChange={(access) => {
              if (access === 'read_only' || access === 'read_write')
                onChange({ code: { ...code, access } });
            }}
          />
          {root.code_sources?.[code.source]?.access === 'read_only' &&
          code.access === 'read_write' ? (
            <p role="alert">{t('factory.codeAccessOverCeiling')}</p>
          ) : null}
          <SelectMenu
            ariaLabel={t('factory.stepCodeRevision')}
            value={code.revision.kind}
            options={[
              { value: 'admitted_base', label: t('factory.codeAdmittedRevision') },
              { value: 'exact_revision', label: t('manualBuild.exactRevision') },
              { value: 'accepted_candidate', label: t('factory.codeAcceptedCandidate') },
            ]}
            onValueChange={(kind) =>
              onChange({
                code: {
                  ...code,
                  revision:
                    kind === 'exact_revision'
                      ? { kind: 'exact_revision', revision: '' }
                      : kind === 'accepted_candidate'
                        ? { kind: 'accepted_candidate', node: '', outcome: '' }
                        : { kind: 'admitted_base' },
                },
              })
            }
          />
          {code.revision.kind === 'exact_revision' ? (
            <label>
              {t('manualBuild.exactRevision')}
              <input
                value={code.revision.revision}
                onChange={(e) =>
                  onChange({
                    code: {
                      ...code,
                      revision: { kind: 'exact_revision', revision: e.target.value },
                    },
                  })
                }
              />
            </label>
          ) : null}
          {code.revision.kind === 'accepted_candidate' ? (
            <>
              <SelectMenu
                ariaLabel={t('factory.codeCandidateStep')}
                value={code.revision.node}
                options={[
                  { value: '', label: t('factory.chooseInput') },
                  ...root.nodes
                    .filter(
                      (item) =>
                        item.key !== node.key &&
                        item.code?.kind === 'source' &&
                        item.code.source === code.source,
                    )
                    .map((item) => ({ value: item.key, label: item.key })),
                ]}
                onValueChange={(key) =>
                  onChange({
                    code: {
                      ...code,
                      revision: { kind: 'accepted_candidate', node: key, outcome: '' },
                    },
                  })
                }
              />
              <SelectMenu
                ariaLabel={t('factory.codeCandidateOutcome')}
                value={code.revision.outcome}
                options={[
                  { value: '', label: t('factory.chooseInput') },
                  ...(
                    root.nodes.find(
                      (item) =>
                        code.revision.kind === 'accepted_candidate' &&
                        item.key === code.revision.node,
                    )?.outcomes ?? []
                  ).map((item) => ({ value: item.key, label: item.key })),
                ]}
                onValueChange={(outcome) => {
                  if (code.revision.kind === 'accepted_candidate')
                    onChange({ code: { ...code, revision: { ...code.revision, outcome } } });
                }}
              />
            </>
          ) : null}
          <CodeScope
            value={code.scope}
            onChange={(scope) => onChange({ code: { ...code, scope } })}
          />
        </>
      ) : null}
      <details>
        <summary>{t('factory.routeDetails')}</summary>
        <p>{t('factory.codeSupportReview')}</p>
      </details>
    </fieldset>
  );
}
