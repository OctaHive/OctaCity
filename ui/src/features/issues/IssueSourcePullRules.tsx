import { ISSUE_SOURCE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { WorkflowResource } from '../../api/issueWorkflows';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { useDraftRowKeys } from '../../shared/useDraftRowKeys';
import type { IssueSourceMappingDefinition, IssueSourceChoice } from './sourceApi';
type Rows = NonNullable<IssueSourceMappingDefinition['status']>['pull'];

/** Opaque native predicates and stable business IDs remain separate values. */
export function IssueSourcePullRules({
  rows,
  choice,
  workflow,
  onChange,
}: {
  rows: Rows;
  choice: IssueSourceChoice;
  workflow: WorkflowResource;
  onChange: (rows: Rows) => void;
}) {
  const { t } = usePresentation();
  const keys = useDraftRowKeys(rows);
  function change(index: number, patch: Partial<Rows[number]>) {
    onChange(rows.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }
  return (
    <section aria-label={t('issueSource.pullAuthority')}>
      {rows.map((row, index) => (
        <fieldset key={keys[index]}>
          <legend>{t('issueSource.pullRule', { number: index + 1 })}</legend>
          <SelectMenu
            ariaLabel={t('issueSource.workflowStatus')}
            value={row.status_id}
            options={[
              { value: '', label: t('issueSource.chooseStatus') },
              ...workflow.definition.statuses.map((s) => ({ value: s.id, label: s.name })),
            ]}
            onValueChange={(status_id) => change(index, { status_id })}
          />
          <SelectMenu
            ariaLabel={t('issueSource.nativeState')}
            value={row.native.state ?? ''}
            options={[
              { value: '', label: t('issueSource.anyState') },
              ...choice.readable.states.map((s) => ({ value: s, label: s })),
            ]}
            onValueChange={(state) =>
              change(index, { native: { ...row.native, state: state || null } })
            }
          />
          {(['all_of', 'any_of', 'none_of'] as const).map((kind) => (
            <fieldset key={kind}>
              <legend>{t(`issueSource.${kind}`, { label: '' })}</legend>
              {choice.readable.labels.map((label) => (
                <label key={label}>
                  <input
                    type="checkbox"
                    aria-label={t(`issueSource.${kind}`, { label })}
                    checked={row.native[kind].includes(label)}
                    disabled={
                      !row.native[kind].includes(label) &&
                      row.native[kind].length >= bounds.predicateMaximumLabels
                    }
                    onChange={(e) =>
                      change(index, {
                        native: {
                          ...row.native,
                          [kind]: e.target.checked
                            ? [...row.native[kind], label]
                            : row.native[kind].filter((v) => v !== label),
                        },
                      })
                    }
                  />
                  {label}
                </label>
              ))}
            </fieldset>
          ))}
          <button type="button" onClick={() => onChange(rows.filter((_, i) => i !== index))}>
            {t('issueSource.removePull')}
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={rows.length >= bounds.pullMaximumRows}
        onClick={() =>
          onChange([
            ...rows,
            { status_id: '', native: { state: null, all_of: [], any_of: [], none_of: [] } },
          ])
        }
      >
        {t('issueSource.addPull')}
      </button>
    </section>
  );
}
