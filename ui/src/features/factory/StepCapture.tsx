import type { components } from '../../../.generated/api/schema';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { MessageKey } from '../../app/presentation/messages';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import { ValueMappingEditor } from './ValueMappingEditor';

type Schema = components['schemas'];
type Node = Schema['FactoryFlowNodeDefinition'];
type Build = Schema['FactoryFlowBuildBinding'];
type Policy = Schema['FactoryDeliverablePolicy'];
type Capture = Pick<Build, 'capture_write_paths' | 'capture_deliverables'>;
const bounds = FACTORY_AUTHORING_SCHEMAS;

// Rust orders repository paths by Unicode scalar value (UTF-8 byte order).
function comparePaths(left: string, right: string) {
  const rightPoints = [...right];
  let index = 0;
  for (const point of left) {
    const other = rightPoints[index++];
    if (other === undefined) return 1;
    if (point !== other) return point.codePointAt(0)! - other.codePointAt(0)!;
  }
  return index - rightPoints.length;
}

/** Capture is a step contract shared by every candidate, independent of executor identity. */
export function StepCapture({
  node,
  onChange,
}: {
  node: Node;
  onChange: (patch: Partial<Node>) => void;
}) {
  const { t } = usePresentation();
  const selection = node.profile_selection;
  const build = node.build ?? Object.values(selection?.candidates ?? {})[0]?.build;
  const deliverables = build?.capture_deliverables;
  const required = deliverables?.policy.required ?? [];
  const rowKeys = useDraftRowKeys(required);
  if (!build) return null;
  const change = (capture: Capture) => {
    if (node.build) onChange({ build: { ...node.build, ...capture } });
    else if (selection)
      onChange({
        profile_selection: {
          ...selection,
          candidates: Object.fromEntries(
            Object.entries(selection.candidates).map(([key, candidate]) => [
              key,
              { ...candidate, build: { ...candidate.build, ...capture } },
            ]),
          ),
        },
      });
  };
  const changePolicy = (patch: Partial<Policy>) => {
    if (deliverables)
      change({
        capture_deliverables: { ...deliverables, policy: { ...deliverables.policy, ...patch } },
      });
  };
  return (
    <details>
      <summary>{t('factory.candidateCapture')}</summary>
      <p>{t('factory.candidateCaptureHelp')}</p>
      <Paths
        label="factory.captureDirectories"
        value={build.capture_write_paths ?? []}
        onChange={(paths) => change({ capture_write_paths: paths.length ? paths : null })}
      />
      <label>
        <input
          type="checkbox"
          checked={!!deliverables}
          onChange={(event) =>
            change({
              capture_deliverables: event.target.checked
                ? {
                    accepted_contract: { value: { mapping: 'pointer', path: '' } },
                    policy: { permitted_paths: [], forbidden_paths: [], required: [] },
                  }
                : null,
            })
          }
        />
        {t('factory.captureDeliverables')}
      </label>
      {deliverables ? (
        <fieldset>
          <legend>{t('factory.captureDeliverables')}</legend>
          <h4>{t('factory.captureAcceptedContract')}</h4>
          <ValueMappingEditor
            value={deliverables.accepted_contract.value}
            onChange={(value) =>
              change({ capture_deliverables: { ...deliverables, accepted_contract: { value } } })
            }
          />
          <Paths
            label="factory.capturePermitted"
            value={deliverables.policy.permitted_paths}
            onChange={(permitted_paths) => changePolicy({ permitted_paths })}
          />
          <Paths
            label="factory.captureForbidden"
            value={deliverables.policy.forbidden_paths}
            onChange={(forbidden_paths) => changePolicy({ forbidden_paths })}
          />
          {required.map((item, index) => {
            const update = (patch: Partial<typeof item>) =>
              changePolicy({
                required: required.map((row, i) => (i === index ? { ...row, ...patch } : row)),
              });
            return (
              <fieldset key={rowKeys[index]}>
                <label>
                  {t('factory.captureKind')}
                  <input
                    value={item.kind}
                    maxLength={bounds.FactoryDeliverableRequirement.properties.kind.maxLength}
                    onChange={(event) => update({ kind: event.target.value })}
                    onBlur={() => {
                      const sorted = [...required].sort((left, right) =>
                        comparePaths(left.kind, right.kind),
                      );
                      if (sorted.some((row, i) => row !== required[i]))
                        changePolicy({ required: sorted });
                    }}
                  />
                </label>
                <label>
                  {t('factory.captureMinimum')}
                  <input
                    type="number"
                    min={bounds.FactoryDeliverableRequirement.properties.min_files.minimum}
                    max={bounds.FactoryDeliverableRequirement.properties.min_files.maximum}
                    value={item.min_files}
                    onChange={(event) => update({ min_files: Number(event.target.value) })}
                  />
                </label>
                <Paths
                  label="factory.captureRequired"
                  value={item.paths}
                  onChange={(paths) => update({ paths })}
                />
                <button
                  type="button"
                  onClick={() => changePolicy({ required: required.filter((_, i) => i !== index) })}
                >
                  {t('factory.removeItem')}
                </button>
              </fieldset>
            );
          })}
          <button
            type="button"
            disabled={
              required.length >= bounds.FactoryDeliverablePolicy.properties.required.maxItems
            }
            onClick={() =>
              changePolicy({
                required: [
                  ...required,
                  {
                    kind: '',
                    min_files: bounds.FactoryDeliverableRequirement.properties.min_files.minimum,
                    paths: [],
                  },
                ],
              })
            }
          >
            {t('factory.captureAddRequirement')}
          </button>
        </fieldset>
      ) : null}
    </details>
  );
}

function Paths({
  label,
  value,
  onChange,
}: {
  label: MessageKey;
  value: readonly string[];
  onChange: (paths: string[]) => void;
}) {
  const { t } = usePresentation();
  return (
    <label>
      {t(label)}
      <textarea
        rows={2}
        value={value.join('\n')}
        onChange={(event) => {
          const paths = event.target.value;
          onChange(paths ? paths.split('\n') : []);
        }}
        onBlur={(event) => {
          const lines = event.target.value.split('\n');
          const paths = lines.filter(Boolean).sort(comparePaths);
          if (paths.join('\n') !== value.join('\n')) onChange(paths);
        }}
      />
    </label>
  );
}
