import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import type { FactoryRolesApi } from './roleApi';

type Ref = components['schemas']['FactoryImmutableReferenceResource'];
type Request = components['schemas']['ImportProjectSkillRequest'];
const same = (a: Ref, b: Ref) =>
  a.identity === b.identity && a.version === b.version && a.digest === b.digest;
function readFile(file: File): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error('File read failed'));
    reader.onload = () =>
      reader.result instanceof ArrayBuffer
        ? resolve(reader.result)
        : reject(new Error('Invalid file'));
    reader.readAsArrayBuffer(file);
  });
}
/** Only an explicit directory selection is read. The ordinary import owns tree validation and retention. */
export function SkillFolderFields({
  api,
  project,
  value,
  onChange,
  available = {},
}: {
  api: FactoryRolesApi;
  project: string;
  value: Record<string, Ref>;
  available?: Record<string, Ref>;
  onChange: (value: Record<string, Ref>) => void;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [request, setRequest] = useState<Request | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [review, setReview] = useState<{ request: Request; target: HTMLElement } | null>(null);
  const [imported, setImported] = useState<Ref[]>([]);
  const skills = useCursorPage(
    queryKeys.factoryRoleCatalog(project, 'skills'),
    async (cursor, signal) => {
      const page = await api.listSkills(project, cursor, signal);
      return {
        ...page,
        items: page.items.map((item) => ({
          ...item,
          id: `${item.identity}:${item.version}:${item.digest}`,
        })),
      };
    },
  );
  const choices = [
    ...Object.entries({ ...available, ...value }).map(([alias, reference]) => ({
      alias,
      reference,
    })),
    ...(skills.data?.pages.flatMap((page) => page.items) ?? []).map((reference) => ({
      alias: reference.identity,
      reference,
    })),
    ...imported.map((reference) => ({ alias: reference.identity, reference })),
  ].filter(
    (item, index, all) =>
      all.findIndex(
        (other) => other.alias === item.alias && same(other.reference, item.reference),
      ) === index,
  );
  async function select(files: File[]) {
    if (!files.length) return;
    setBusy(true);
    setError(null);
    setRequest(null);
    try {
      const root = files[0]!.webkitRelativePath.split('/')[0];
      if (!root || files.some((file) => !file.webkitRelativePath.startsWith(`${root}/`)))
        throw new Error('Choose one folder');
      // The wire limit includes JSON encoding. This preliminary bound avoids reading oversized folders.
      if (files.reduce((size, file) => size + file.size, 0) > 8 * 1024 * 1024)
        throw new Error('Folder exceeds request limit');
      const entries: Request['files'] = [];
      for (const file of files) {
        const bytes = new Uint8Array(await readFile(file));
        const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
        entries.push({
          path: file.webkitRelativePath.slice(root.length + 1),
          bytes: Array.from(bytes),
          sha256: Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join(''),
        });
      }
      const next = { identity: root, files: entries };
      if (new TextEncoder().encode(JSON.stringify(next)).length > 8 * 1024 * 1024)
        throw new Error('Folder exceeds request limit');
      setRequest(next);
    } catch (failure) {
      setError(failure instanceof Error ? failure : new Error());
    } finally {
      setBusy(false);
    }
  }
  const label = t('factory.skills');
  return (
    <section>
      <fieldset disabled={busy || !!review}>
        <legend>{label}</legend>
        <label>
          {t('factory.importFolder')}
          <input
            ref={(element) => element?.setAttribute('webkitdirectory', '')}
            type="file"
            multiple
            disabled={busy || !!review}
            onChange={(event) => {
              const files = Array.from(event.target.files ?? []);
              event.target.value = '';
              void select(files);
            }}
          />
        </label>
        {busy ? <p role="status">{t('factory.readingFolder')}</p> : null}
        {error ? (
          <QueryFailureNotice
            title={t('factory.importFolder')}
            error={error}
            onRetry={() => setError(null)}
          />
        ) : null}
        <PagedExplorerBranch
          query={skills}
          empty={t('factory.noSkills')}
          labels={{
            failure: label,
            stale: label,
            loading: label,
            loadMore: label,
            loadingMore: label,
          }}
        >
          {() => null}
        </PagedExplorerBranch>
        {choices.map(({ alias, reference }) => (
          <label key={`${alias}:${reference.identity}:${reference.version}:${reference.digest}`}>
            <input
              type="checkbox"
              checked={!!value[alias] && same(value[alias]!, reference)}
              onChange={(event) => {
                const next = { ...value };
                if (event.target.checked) next[alias] = reference;
                else delete next[alias];
                onChange(next);
              }}
            />
            {alias !== reference.identity ? `${alias} · ` : ''}
            {reference.identity} · {reference.version} · {reference.digest}
          </label>
        ))}
        {request ? (
          <section>
            <label>
              {t('factory.skillIdentity')}
              <input
                value={request.identity}
                onChange={(event) => setRequest({ ...request, identity: event.target.value })}
              />
            </label>
            <ul>
              {request.files.map((file) => (
                <li key={file.path}>
                  {file.path} · {file.bytes.length} bytes
                </li>
              ))}
            </ul>
            <button
              type="button"
              onClick={(event) =>
                setReview({ target: event.currentTarget, request: structuredClone(request) })
              }
            >
              {t('factory.reviewImport')}
            </button>
          </section>
        ) : null}
      </fieldset>
      {review ? (
        <ConfirmedCommand
          title={t('factory.reviewImport')}
          confirmLabel={t('factory.confirmImport')}
          request={review.request}
          consequence={`${review.request.identity} · ${review.request.files.map((file) => file.path).join(' · ')}`}
          returnFocus={review.target}
          queryClient={client}
          invalidations={[
            { queryKey: queryKeys.factoryRoleCatalog(project, 'skills') },
            { queryKey: queryKeys.audit },
          ]}
          execute={async (attempt) => {
            const receipt = await api.importSkill(
              project,
              structuredClone(attempt.request) as Request,
              attempt.headers,
            );
            setImported((current) => [
              ...current.filter((item) => !same(item, receipt.bundle)),
              receipt.bundle,
            ]);
            return receipt;
          }}
          onClose={() => setReview(null)}
          renderSuccess={() => <p>{t('factory.skillImported')}</p>}
        />
      ) : null}
    </section>
  );
}
