import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { ValueMappingEditor } from './ValueMappingEditor';
import { PredicateEditor } from './PredicateEditor';
import { ProfileCandidatePicker } from './ProfileCandidatePicker';
import type { FactoryRolesApi } from './roleApi';

type Schema = components['schemas'];
type Binding = Schema['FactoryProfileSelectionBinding'];
type Node = Schema['FactoryFlowNodeDefinition'];
export type RoleServices = { api: FactoryRolesApi; projectId: string };
/** Selection is declarative. The existing Flow validation owns compatibility and provider qualification. */
export function StageProfiles({
  node,
  onChange,
  choices,
  services,
}: {
  node: Node;
  onChange: (patch: Partial<Node>) => void;
  choices: readonly Schema['FactoryConfigurationChoiceResource'][];
  services?: RoleServices | undefined;
}) {
  const { t } = usePresentation();
  const [adding, setAdding] = useState(false);
  const binding = node.profile_selection;
  const rules = binding?.policy.mode === 'rules' ? binding.policy.rules : [];
  const ruleKeys = useDraftRowKeys(rules);
  if (node.kind !== 'reasoning') return null;
  const execution = node.build ?? Object.values(binding?.candidates ?? {})[0]?.build;
  const update = (patch: Partial<Binding>) =>
    binding && onChange({ profile_selection: { ...binding, ...patch } });
  const candidates = Object.keys(binding?.candidates ?? {}).map((name) => ({
    value: name,
    label: name,
  }));
  const first = candidates[0]?.value ?? '';
  const policies = choices.filter(
    (choice) =>
      choice.kind === 'decision_signal_policy' &&
      choice.metadata?.signal_purposes.includes('profile_selection') &&
      choice.reference.kind === 'immutable',
  );
  const policyOptions = policies.flatMap((choice) =>
    choice.reference.kind === 'immutable'
      ? [
          {
            value: choice.reference.reference.digest,
            label: `${choice.alias} · ${choice.reference.reference.version}`,
          },
        ]
      : [],
  );
  const policy = binding?.policy;
  const installation =
    policy?.mode === 'decision_api'
      ? policies.find(
          (choice) =>
            choice.reference.kind === 'immutable' &&
            choice.reference.reference.digest === policy.policy.digest,
        )?.metadata?.profile_selection
      : undefined;
  return (
    <section>
      <h4>{t('factory.stageProfiles')}</h4>
      {services && execution ? (
        <button type="button" onClick={() => setAdding(!adding)}>
          {t('factory.addProfileCandidate')}
        </button>
      ) : null}
      {adding && services && execution ? (
        <ProfileCandidatePicker
          services={services}
          onSelect={(profile) => {
            if (!execution) return;
            if (
              Object.values(binding?.candidates ?? {}).some(
                (candidate) =>
                  candidate.profile.reference.id === profile.reference.id &&
                  candidate.profile.reference.version === profile.reference.version,
              )
            ) {
              setAdding(false);
              return;
            }
            const alias = profile.reference.id;
            const build = {
              ...structuredClone(execution),
              plugin: profile.definition.toolchain.plugin,
              tool: profile.definition.toolchain.executable,
              model_or_tool: profile.definition.toolchain.model,
              build_configuration: profile.definition.build_configuration,
            };
            onChange({
              build: null,
              profile_selection: binding
                ? { ...binding, candidates: { ...binding.candidates, [alias]: { profile, build } } }
                : {
                    role: profile.role,
                    candidates: { [alias]: { profile, build } },
                    policy: { mode: 'manual', candidate: null },
                    failure: { kind: 'manual_selection' },
                    selection_budget: {
                      attempts: 0,
                      elapsed_millis: 0,
                      tokens: 0,
                      cost_micro_units: 0,
                      output_bytes: 0,
                    },
                  },
            });
            setAdding(false);
          }}
        />
      ) : null}
      {binding ? (
        <>
          <ul>
            {Object.entries(binding.candidates).map(([alias, candidate]) => (
              <li key={`${candidate.profile.reference.id}:${candidate.profile.reference.version}`}>
                {candidate.profile.definition.name} · v{candidate.profile.reference.version}
                {installation ? (
                  <SelectMenu
                    ariaLabel={t('factory.selectorAnswer', {
                      profile: candidate.profile.definition.name,
                    })}
                    value={alias}
                    options={[
                      ...(!installation.candidates.includes(alias)
                        ? [{ value: alias, label: alias }]
                        : []),
                      ...installation.candidates
                        .filter((answer) => answer === alias || !binding.candidates[answer])
                        .map((answer) => ({ value: answer, label: answer })),
                    ]}
                    onValueChange={(answer) => {
                      if (answer === alias || binding.candidates[answer]) return;
                      update({
                        candidates: Object.fromEntries(
                          Object.entries(binding.candidates).map(([key, value]) => [
                            key === alias ? answer : key,
                            value,
                          ]),
                        ),
                        failure:
                          binding.failure.kind === 'reserve_profile' &&
                          binding.failure.candidate === alias
                            ? { kind: 'reserve_profile', candidate: answer }
                            : binding.failure,
                      });
                    }}
                  />
                ) : null}
                <button
                  type="button"
                  onClick={() => {
                    const next = { ...binding.candidates };
                    delete next[alias];
                    update({ candidates: next });
                  }}
                >
                  {t('factory.removeCandidate', { name: alias })}
                </button>
              </li>
            ))}
          </ul>
          <SelectMenu
            ariaLabel={t('factory.profileMode')}
            value={policy?.mode ?? 'manual'}
            options={[
              { value: 'manual', label: t('factory.manualSelection') },
              { value: 'rules', label: t('factory.orderedRules') },
              { value: 'decision_api', label: t('factory.decisionApi') },
            ]}
            onValueChange={(mode) => {
              if (mode === 'manual') update({ policy: { mode, candidate: null } });
              if (mode === 'rules') update({ policy: { mode, rules: [], otherwise: null } });
              if (mode === 'decision_api') {
                const choice = policies[0];
                if (choice?.reference.kind === 'immutable')
                  update({
                    policy: {
                      mode,
                      policy: choice.reference.reference,
                      input: { value: { mapping: 'object', fields: {} } },
                    },
                  });
                else setAdding(true);
              }
            }}
          />
          {adding && !policies.length ? <p>{t('factory.noProfilePolicy')}</p> : null}
          {policy?.mode === 'manual' ? (
            <SelectMenu
              ariaLabel={t('factory.baselineProfile')}
              value={policy.candidate ?? ''}
              options={[{ value: '', label: t('factory.waitForProfile') }, ...candidates]}
              onValueChange={(candidate) =>
                update({ policy: { mode: 'manual', candidate: candidate || null } })
              }
            />
          ) : null}
          {policy?.mode === 'rules' ? (
            <>
              <button
                type="button"
                disabled={
                  rules.length >=
                    FACTORY_AUTHORING_SCHEMAS.FactoryProfileSelectionPolicy.oneOf[1].properties
                      .rules.maxItems || !first
                }
                onClick={() =>
                  update({
                    policy: {
                      ...policy,
                      rules: [
                        ...rules,
                        {
                          candidate: first,
                          when: { operation: 'exists', operand: { source: 'path', path: '' } },
                        },
                      ],
                    },
                  })
                }
              >
                {t('factory.addSelectionRule')}
              </button>
              {rules.map((rule, index) => (
                <fieldset key={ruleKeys[index]}>
                  <legend>{t('factory.selectionRule', { index: index + 1 })}</legend>
                  <PredicateEditor
                    value={rule.when}
                    onChange={(when) =>
                      update({
                        policy: {
                          ...policy,
                          rules: rules.map((item, at) => (at === index ? { ...item, when } : item)),
                        },
                      })
                    }
                  />
                  <SelectMenu
                    ariaLabel={t('factory.ruleProfile')}
                    value={rule.candidate}
                    options={candidates}
                    onValueChange={(candidate) =>
                      update({
                        policy: {
                          ...policy,
                          rules: rules.map((item, at) =>
                            at === index ? { ...item, candidate } : item,
                          ),
                        },
                      })
                    }
                  />
                  <button
                    type="button"
                    disabled={!index}
                    onClick={() => {
                      const next = [...rules];
                      [next[index - 1], next[index]] = [next[index]!, next[index - 1]!];
                      update({ policy: { ...policy, rules: next } });
                    }}
                  >
                    {t('factory.moveRule')}
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      update({
                        policy: { ...policy, rules: rules.filter((_, at) => at !== index) },
                      })
                    }
                  >
                    {t('factory.removeRule')}
                  </button>
                </fieldset>
              ))}
              <SelectMenu
                ariaLabel={t('factory.otherwiseProfile')}
                value={policy.otherwise ?? ''}
                options={[{ value: '', label: t('factory.none') }, ...candidates]}
                onValueChange={(otherwise) =>
                  update({ policy: { ...policy, otherwise: otherwise || null } })
                }
              />
            </>
          ) : null}
          {policy?.mode === 'decision_api' ? (
            <>
              <SelectMenu
                ariaLabel={t('factory.selectorPolicy')}
                value={policy.policy.digest}
                options={policyOptions}
                onValueChange={(digest) => {
                  const choice = policies.find(
                    (item) =>
                      item.reference.kind === 'immutable' &&
                      item.reference.reference.digest === digest,
                  );
                  if (choice?.reference.kind === 'immutable')
                    update({ policy: { ...policy, policy: choice.reference.reference } });
                }}
              />
              <p>{t('factory.installedPolicyHelp')}</p>
              {installation ? (
                <section aria-label={t('factory.selectorQualification')}>
                  <h5>{t('factory.selectorQualification')}</h5>
                  <p>{t('factory.selectorAnswerHelp')}</p>
                  <dl>
                    <div>
                      <dt>{t('factory.selectorRollout')}</dt>
                      <dd>{installation.mode}</dd>
                    </div>
                    <div>
                      <dt>{t('factory.minimumProbability')}</dt>
                      <dd>{installation.minimum_probability}</dd>
                    </div>
                    <div>
                      <dt>{t('factory.minimumMargin')}</dt>
                      <dd>{installation.minimum_margin}</dd>
                    </div>
                    <div>
                      <dt>{t('factory.baselineProfile')}</dt>
                      <dd>{installation.baseline}</dd>
                    </div>
                  </dl>
                  <ul>
                    {(
                      [
                        'provider',
                        'adapter',
                        'model',
                        'question_set',
                        'calibration',
                        'probability_semantics',
                      ] as const
                    ).map((key) => (
                      <li key={key}>
                        {installation[key].identity} · {installation[key].version}
                      </li>
                    ))}
                  </ul>
                </section>
              ) : (
                <p>{t('factory.selectorMetadataUnavailable')}</p>
              )}

              <ul>
                {policies
                  .find(
                    (choice) =>
                      choice.reference.kind === 'immutable' &&
                      choice.reference.reference.digest === policy.policy.digest,
                  )
                  ?.metadata?.compatible_choices.map((item) => (
                    <li key={`${item.kind}:${item.alias}`}>{item.alias}</li>
                  ))}
              </ul>
              <ValueMappingEditor
                value={policy.input.value}
                onChange={(value) => update({ policy: { ...policy, input: { value } } })}
              />
            </>
          ) : null}
          <SelectMenu
            ariaLabel={t('factory.selectionFailure')}
            value={binding.failure.kind}
            options={[
              { value: 'manual_selection', label: t('factory.manualSelection') },
              { value: 'reserve_profile', label: t('factory.reserveProfile') },
            ]}
            onValueChange={(kind) =>
              update({
                failure:
                  kind === 'reserve_profile'
                    ? { kind, candidate: first }
                    : { kind: 'manual_selection' },
              })
            }
          />
          {binding.failure.kind === 'reserve_profile' ? (
            <SelectMenu
              ariaLabel={t('factory.reserveProfile')}
              value={binding.failure.candidate}
              options={candidates}
              onValueChange={(candidate) =>
                update({ failure: { kind: 'reserve_profile', candidate } })
              }
            />
          ) : null}
          <fieldset>
            <legend>{t('factory.selectionBudget')}</legend>
            {(Object.keys(binding.selection_budget) as (keyof Binding['selection_budget'])[]).map(
              (key) => (
                <label key={key}>
                  {key}
                  <input
                    type="number"
                    min={0}
                    value={binding.selection_budget[key]}
                    onChange={(event) =>
                      update({
                        selection_budget: {
                          ...binding.selection_budget,
                          [key]: Number(event.target.value),
                        },
                      })
                    }
                  />
                </label>
              ),
            )}
          </fieldset>
          <button
            type="button"
            onClick={() => onChange({ profile_selection: null, build: execution ?? null })}
          >
            {t('factory.removeProfileSelection')}
          </button>
        </>
      ) : null}
    </section>
  );
}
