import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type Root = components['schemas']['FactoryFlowRootDraft'];
type Edge = Pick<Root['context_projections'][number], 'predecessor' | 'outcome' | 'successor'>;
/** Three independent edge sets: a control dependency never creates a projection. */
export function FlowProjections({
  value,
  onChange,
}: {
  value: Root;
  onChange: (root: Root) => void;
}) {
  const { t } = usePresentation();
  const first = value.nodes[0];
  const successor = value.nodes[1] ?? first;
  const contextKeys = useDraftRowKeys(value.context_projections);
  const dataKeys = useDraftRowKeys(value.data_projections);
  const updateContext = (index: number, patch: Partial<Root['context_projections'][number]>) =>
    onChange({
      ...value,
      context_projections: value.context_projections.map((edge, position) =>
        position === index ? { ...edge, ...patch } : edge,
      ),
    });
  const updateData = (index: number, patch: Partial<Root['data_projections'][number]>) =>
    onChange({
      ...value,
      data_projections: value.data_projections.map((edge, position) =>
        position === index ? { ...edge, ...patch } : edge,
      ),
    });
  return (
    <section data-factory-field="/root/context_projections" tabIndex={-1}>
      <h4>{t('factory.projections')}</h4>
      <h5>{t('factory.contextProjections')}</h5>
      <button
        type="button"
        disabled={!first?.outcomes.length}
        onClick={() => {
          if (first && successor)
            onChange({
              ...value,
              context_projections: [
                ...value.context_projections,
                {
                  predecessor: first.key,
                  outcome: first.outcomes[0]?.key ?? '',
                  successor: successor.key,
                  selections: [],
                },
              ],
            });
        }}
      >
        {t('factory.addContextProjection')}
      </button>
      {value.context_projections.map((edge, index) => {
        const node = value.nodes.find((node) => node.key === edge.predecessor);
        const selections = Object.keys(node?.context_binding?.entries ?? {});
        return (
          <fieldset key={contextKeys[index]}>
            <EdgeFields
              value={edge}
              root={value}
              onChange={(patch) =>
                updateContext(index, {
                  ...patch,
                  ...(patch.predecessor === undefined ? {} : { selections: [] }),
                })
              }
            />
            {selections.map((name) => (
              <label key={name}>
                <input
                  type="checkbox"
                  checked={edge.selections.includes(name)}
                  onChange={(event) =>
                    updateContext(index, {
                      selections: event.target.checked
                        ? [...edge.selections, name]
                        : edge.selections.filter((item) => item !== name),
                    })
                  }
                />
                {name}
              </label>
            ))}
            <button
              type="button"
              onClick={() =>
                onChange({
                  ...value,
                  context_projections: value.context_projections.filter(
                    (_, position) => position !== index,
                  ),
                })
              }
            >
              {t('factory.removeProjection')}
            </button>
          </fieldset>
        );
      })}
      <h5>{t('factory.dataProjections')}</h5>
      <button
        type="button"
        disabled={!first?.outcomes.length}
        onClick={() => {
          const outcome = first?.outcomes[0];
          if (first && successor && outcome)
            onChange({
              ...value,
              data_projections: [
                ...value.data_projections,
                {
                  predecessor: first.key,
                  outcome: outcome.key,
                  successor: successor.key,
                  schema: outcome.schema,
                },
              ],
            });
        }}
      >
        {t('factory.addDataProjection')}
      </button>
      {value.data_projections.map((edge, index) => (
        <fieldset key={dataKeys[index]}>
          <EdgeFields
            value={edge}
            root={value}
            onChange={(patch) => {
              const predecessor = patch.predecessor ?? edge.predecessor;
              const outcome = value.nodes
                .find((node) => node.key === predecessor)
                ?.outcomes.find((outcome) => outcome.key === (patch.outcome ?? edge.outcome));
              updateData(index, { ...patch, ...(outcome ? { schema: outcome.schema } : {}) });
            }}
          />
          <p>
            {edge.schema.identity} · {edge.schema.version}
          </p>
          <button
            type="button"
            onClick={() =>
              onChange({
                ...value,
                data_projections: value.data_projections.filter(
                  (_, position) => position !== index,
                ),
              })
            }
          >
            {t('factory.removeProjection')}
          </button>
        </fieldset>
      ))}
    </section>
  );
}
function EdgeFields({
  value,
  root,
  onChange,
}: {
  value: Edge;
  root: Root;
  onChange: (value: Partial<Edge>) => void;
}) {
  const { t } = usePresentation();
  const nodes = root.nodes.map((node) => ({ value: node.key, label: node.key }));
  return (
    <>
      <SelectMenu
        ariaLabel={t('factory.predecessor')}
        value={value.predecessor}
        options={nodes}
        onValueChange={(predecessor) =>
          onChange({
            predecessor,
            outcome: root.nodes.find((node) => node.key === predecessor)?.outcomes[0]?.key ?? '',
          })
        }
      />
      <SelectMenu
        ariaLabel={t('factory.transitionOutcome')}
        value={value.outcome}
        options={(root.nodes.find((node) => node.key === value.predecessor)?.outcomes ?? []).map(
          (outcome) => ({ value: outcome.key, label: outcome.key }),
        )}
        onValueChange={(outcome) => onChange({ outcome })}
      />
      <SelectMenu
        ariaLabel={t('factory.successor')}
        value={value.successor}
        options={nodes}
        onValueChange={(successor) => onChange({ successor })}
      />
    </>
  );
}
