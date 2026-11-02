import { useId, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { useOverlayFocus } from '../../shared/useOverlayFocus';
import { conflictingMounts, mergePermissions, requiredPermissions } from './flowRequirements';
import { PermissionEditor } from './ResourceEditors';
import styles from '../../shared/ConfirmedCommand.module.css';
import layout from './Factory.module.css';

type Draft = components['schemas']['FactoryFlowDefinitionDraft'];
/** Explicitly reviews one exact draft change. Opening/cancelling never requests access. */
export function AccessRequirements({
  value,
  onChange,
}: {
  value: Draft;
  onChange: (value: Draft) => void;
}) {
  const { t } = usePresentation();
  const [review, setReview] = useState<{
    baseline: string;
    draft: Draft;
  } | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null),
    cancelRef = useRef<HTMLButtonElement>(null);
  const sectionRef = useRef<HTMLElement>(null),
    triggerRef = useRef<HTMLButtonElement>(null);
  const headingId = useId();
  useOverlayFocus({
    active: !!review,
    containerRef: dialogRef,
    initialFocusRef: dialogRef,
    onDismiss: () => setReview(null),
    restoreFocus: () =>
      (triggerRef.current?.disabled ? sectionRef.current : triggerRef.current)?.focus(),
  });
  const nodes = value.root.nodes.map((node) => ({
    ...node,
    permissions: requiredPermissions(node),
  }));
  const permissions = nodes.reduce(
    (permissions, node) => mergePermissions(permissions, node.permissions),
    value.root.execution.permissions,
  );
  const proposed: Draft = {
    ...value,
    root: { ...value.root, nodes, execution: { ...value.root.execution, permissions } },
    limits: {
      ...value.limits,
      permissions: mergePermissions(value.limits.permissions, permissions),
    },
  };
  const changed = JSON.stringify(proposed) !== JSON.stringify(value);
  const conflict =
    !!review &&
    [
      ...review.draft.root.nodes.map((node) => node.permissions),
      review.draft.root.execution.permissions,
      review.draft.limits.permissions,
    ].some(conflictingMounts);
  return (
    <section className={layout.editor} ref={sectionRef} tabIndex={-1}>
      <p>{t('factory.accessRequirementsHelp')}</p>
      <div className={layout.definitionActions}>
        <button
          ref={triggerRef}
          type="button"
          disabled={!changed}
          onClick={() =>
            setReview({
              baseline: JSON.stringify(value),
              draft: structuredClone(proposed),
            })
          }
        >
          {t('factory.reviewRequiredAccess')}
        </button>
      </div>
      {review ? (
        <div className={styles.layer}>
          <div
            ref={dialogRef}
            className={styles.dialog}
            role="dialog"
            tabIndex={-1}
            aria-modal="true"
            aria-labelledby={headingId}
          >
            <h2 id={headingId}>{t('factory.requiredAccess')}</h2>
            <p>{t('factory.accessRequirementsHelp')}</p>
            {[
              ...review.draft.root.nodes.map((node) => ({
                label: `${t('factory.node')} · ${node.key}`,
                permissions: node.permissions,
              })),
              {
                label: t('factory.executionPermissions'),
                permissions: review.draft.root.execution.permissions,
              },
              {
                label: t('factory.admissionPermissions'),
                permissions: review.draft.limits.permissions,
              },
            ].map((item, index) => (
              <details key={item.label} open={index < review.draft.root.nodes.length}>
                <summary>{item.label}</summary>
                <p>
                  {t('factory.permissionCounts', {
                    plugins: item.permissions.plugins.length,
                    tools: item.permissions.tools.length,
                    hosts: item.permissions.network_hosts.length,
                  })}
                </p>
                <ul>
                  {item.permissions.network_hosts.map((host) => (
                    <li key={host}>
                      {t('factory.networkHost')} · {host}
                    </li>
                  ))}
                  {item.permissions.secret_profiles.map((name) => (
                    <li key={name}>
                      {t('factory.secretProfiles')} · {name}
                    </li>
                  ))}
                </ul>
                <details>
                  <summary>{t('factory.exactAccessFields')}</summary>
                  <fieldset disabled>
                    <PermissionEditor
                      label={item.label}
                      value={item.permissions}
                      ceiling={item.permissions}
                      choices={item.permissions}
                      onChange={() => {}}
                    />
                  </fieldset>
                </details>
              </details>
            ))}
            {review.baseline !== JSON.stringify(value) ? (
              <p role="alert">{t('factory.accessReviewStale')}</p>
            ) : null}
            {conflict ? <p role="alert">{t('factory.accessMountConflict')}</p> : null}
            <div className={styles.actions}>
              <button ref={cancelRef} type="button" onClick={() => setReview(null)}>
                {t('factory.cancelSetup')}
              </button>
              <button
                type="button"
                disabled={conflict || review.baseline !== JSON.stringify(value)}
                onClick={() => {
                  onChange(review.draft);
                  setReview(null);
                }}
              >
                {t('factory.applyToDraft')}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
