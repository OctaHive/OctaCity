import { useEffect, useRef, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { idempotencyHeaders } from '../../api/client';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { PagedExplorerBranch, useCursorPage } from '../../shared/PagedExplorerBranch';
import { SelectMenu } from '../../shared/SelectMenu';
import { formatTimestamp } from '../../shared/display';
import type { CodeResourceServices } from './FlowCodeSources';
import { catalogLabels } from './catalogLabels';
import type { FlowDocumentServices } from './FlowDocuments';
import type { FlowAuthoringServices } from './FlowPrimitives';
import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';

type Node = components['schemas']['FactoryFlowNodeDefinition'];
type OctaTask = { id: string; path: string; task: string };
const lines = (text: string) =>
  text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
const MAX_FILES = 64;
const MAX_BYTES = 256 * 1024;
const MAX_OCTA_TASK_BYTES = 128;

function isLiteralOctaTask(task: string): boolean {
  return (
    !/[*\p{Cc}]/u.test(task) &&
    task.split(':').every(Boolean) &&
    new TextEncoder().encode(task).length <= MAX_OCTA_TASK_BYTES
  );
}

function uploadBytes(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => {
      const value = reader.result;
      if (typeof value === 'string') resolve(value.slice(value.indexOf(',') + 1));
      else reject(new Error());
    });
    reader.addEventListener('error', () => reject(reader.error));
    reader.readAsDataURL(file);
  });
}

type Authors = NonNullable<
  components['schemas']['FactoryAuthoringCapabilities']['verification_authors']
>;
const sameReference = (
  left: components['schemas']['FactoryImmutableReferenceResource'],
  right: components['schemas']['FactoryImmutableReferenceResource'],
) =>
  left.identity === right.identity &&
  left.version === right.version &&
  left.digest === right.digest;

function canPrepareVerification(node: Node, authors: Authors | undefined) {
  const bindings = [
    node.build,
    ...Object.values(node.profile_selection?.candidates ?? {}).map(({ build }) => build),
  ].filter((binding) => binding != null);
  return (
    node.kind === 'reasoning' &&
    bindings.length > 0 &&
    bindings.every((binding) =>
      authors?.some((author) => {
        const config = binding.build_configuration;
        const installed = author.build_configuration;
        const inventory = author.inventory;
        return (
          config.id === installed.id &&
          config.project_id === installed.project_id &&
          config.version === installed.version &&
          config.definition_digest === installed.definition_digest &&
          sameReference(binding.tool, author.tool) &&
          sameReference(binding.plugin, author.plugin) &&
          sameReference(binding.model_or_tool, author.model_or_tool) &&
          (author.task_digest === undefined || author.task_digest === binding.task_digest) &&
          binding.result_output === author.result_output &&
          binding.result_output_kind === 'report' &&
          binding.evidence?.some(
            (evidence) =>
              evidence.kind === inventory.kind &&
              evidence.output_kind === inventory.output_kind &&
              sameReference(evidence.schema, inventory.schema) &&
              sameReference(evidence.tool, inventory.tool) &&
              sameReference(evidence.plugin, inventory.plugin),
          )
        );
      }),
    )
  );
}

/** Freezes only operator-selected bytes in the existing private Project input owner. */
export function StepVerification({
  node,
  authoring,
  documents,
  code,
  onChange,
  onPendingChange,
}: {
  node: Node;
  authoring: FlowAuthoringServices | undefined;
  documents: FlowDocumentServices;
  code: CodeResourceServices | undefined;
  onChange: (patch: Pick<Node, 'protected_verification'>) => void;
  onPendingChange: (pending: boolean) => void;
}) {
  const { t, locale } = usePresentation();
  const { data: capabilities } = useFactoryAuthoringCapabilities(authoring);
  const canPrepare = canPrepareVerification(node, capabilities?.verification_authors);
  const [source, setSource] = useState('upload');
  const [files, setFiles] = useState<File[]>([]);
  const [repository, setRepository] =
    useState<components['schemas']['RepositorySummaryResource']>();
  const [revision, setRevision] = useState('');
  const [paths, setPaths] = useState('');
  const [shell, setShell] = useState('');
  const [tasks, setTasks] = useState<OctaTask[]>([]);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [receipt, setReceipt] = useState<components['schemas']['ProjectDocumentResource']>();
  const mountedRef = useRef(true);
  const publicationRef = useRef<{ name: string; content: string; key: string } | null>(null);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  useEffect(() => {
    if (!busy) return;
    onPendingChange(true);
    return () => onPendingChange(false);
  }, [busy, onPendingChange]);
  const repositories = useCursorPage(
    queryKeys.factoryWorkSources(documents.projectId, 'repositories'),
    (cursor, signal) => code!.repositories.listRepositories(documents.projectId, cursor, signal),
    { enabled: !!code && source === 'repository' },
  );
  const upload = files.map((file) => ({ file, path: file.webkitRelativePath || file.name }));
  const selectedPaths = source === 'upload' ? upload.map(({ path }) => path) : lines(paths);
  const entries = [
    ...lines(shell).map((path) => ({ kind: 'shell' as const, path })),
    ...tasks.map(({ path, task }) => ({ kind: 'octafile' as const, path, task })),
  ];
  const withinLimit =
    selectedPaths.length <= MAX_FILES &&
    (source !== 'upload' || files.reduce((total, file) => total + file.size, 0) <= MAX_BYTES);
  const ready =
    withinLimit &&
    selectedPaths.length > 0 &&
    entries.length > 0 &&
    entries.length <= MAX_FILES &&
    entries.every(
      (entry) =>
        selectedPaths.includes(entry.path) &&
        (entry.kind === 'shell' ? entry.path.endsWith('.sh') : isLiteralOctaTask(entry.task)),
    ) &&
    (source === 'upload' || (!!repository && !!revision.trim()));
  async function freeze() {
    if (!ready || busy) return;
    setBusy(true);
    setFailed(false);
    try {
      const input =
        source === 'upload'
          ? {
              files: await Promise.all(
                upload.map(async ({ file, path }) => ({
                  path,
                  content_base64: await uploadBytes(file),
                })),
              ),
            }
          : {
              repository: {
                id: repository!.id,
                version: repository!.version,
                revision: revision.trim(),
                paths: selectedPaths,
              },
            };
      const name = t('factory.verificationTitle');
      const content = JSON.stringify({ ...input, entries });
      if (publicationRef.current?.name !== name || publicationRef.current.content !== content)
        publicationRef.current = { name, content, key: crypto.randomUUID() };
      const result = await documents.api.publish(
        documents.projectId,
        { name, purpose: 'verification', content },
        idempotencyHeaders(publicationRef.current.key),
      );
      if (result.project_id !== documents.projectId) throw new Error();
      publicationRef.current = null;
      if (mountedRef.current) {
        setReceipt(result);
        onChange({ protected_verification: result.artifact });
      }
    } catch {
      if (mountedRef.current) setFailed(true);
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }
  function changeTask(id: string, patch: Partial<OctaTask>) {
    setTasks(tasks.map((task) => (task.id === id ? { ...task, ...patch } : task)));
  }
  if (!canPrepare && !node.protected_verification) return null;
  return (
    <fieldset disabled={busy}>
      <legend>{t('factory.verificationTitle')}</legend>
      {busy ? <p role="status">{t('factory.verificationFreezing')}</p> : null}
      <p>{t('factory.verificationHelp')}</p>
      {node.protected_verification ? (
        <p>
          {t('factory.verificationFrozen')}{' '}
          {receipt?.artifact.artifact_id === node.protected_verification.artifact_id
            ? t('factory.verificationExpiresAt', {
                date: formatTimestamp(receipt.expires_at, locale).display,
              })
            : t('factory.verificationRetentionUnavailable')}{' '}
          <button type="button" onClick={() => onChange({ protected_verification: null })}>
            {t('factory.verificationDetach')}
          </button>
        </p>
      ) : null}
      {canPrepare ? (
        <>
          {code ? (
            <SelectMenu
              ariaLabel={t('factory.verificationSource')}
              value={source}
              options={[
                { value: 'upload', label: t('factory.verificationUpload') },
                { value: 'repository', label: t('factory.projectRepository') },
              ]}
              onValueChange={setSource}
            />
          ) : null}
          {source === 'upload' ? (
            <>
              <label>
                {t('factory.verificationFiles')}
                <input
                  type="file"
                  multiple
                  onChange={(event) => setFiles(Array.from(event.target.files ?? []))}
                />
              </label>
              <label>
                {t('factory.verificationFolder')}
                <input
                  type="file"
                  multiple
                  {...{ webkitdirectory: '' }}
                  onChange={(event) => setFiles(Array.from(event.target.files ?? []))}
                />
              </label>
              {selectedPaths.length ? (
                <ul>
                  {[...new Set(selectedPaths)].slice(0, MAX_FILES).map((path) => (
                    <li key={path}>{path}</li>
                  ))}
                </ul>
              ) : null}
            </>
          ) : (
            <>
              <PagedExplorerBranch
                query={repositories}
                empty={t('factory.noCodeRepositories')}
                labels={catalogLabels(t('factory.projectRepository'))}
              >
                {(items) => {
                  const visible = items.filter((item) => item.project_id === documents.projectId);
                  return (
                    <SelectMenu
                      ariaLabel={t('factory.projectRepository')}
                      value={repository ? `${repository.id}:${repository.version}` : ''}
                      options={[
                        { value: '', label: t('factory.chooseInput') },
                        ...visible.map((item) => ({
                          value: `${item.id}:${item.version}`,
                          label: `${item.name} · v${item.version}`,
                        })),
                      ]}
                      onValueChange={(value) =>
                        setRepository(
                          visible.find((item) => `${item.id}:${item.version}` === value),
                        )
                      }
                    />
                  );
                }}
              </PagedExplorerBranch>
              {repository ? (
                <>
                  <label>
                    {t('manualBuild.exactRevision')}
                    <input value={revision} onChange={(event) => setRevision(event.target.value)} />
                  </label>
                  <label>
                    {t('factory.verificationPaths')}
                    <textarea value={paths} onChange={(event) => setPaths(event.target.value)} />
                  </label>
                </>
              ) : null}
            </>
          )}
          {selectedPaths.length ? (
            <>
              <label>
                {t('factory.verificationShell')}
                <textarea value={shell} onChange={(event) => setShell(event.target.value)} />
              </label>
              {tasks.map(({ id, path, task }) => (
                <fieldset key={id}>
                  <legend>{t('factory.verificationOctaTask')}</legend>
                  <SelectMenu
                    ariaLabel={t('factory.verificationOctafile')}
                    value={path}
                    options={[
                      { value: '', label: t('factory.chooseInput') },
                      ...[...new Set(selectedPaths)].map((value) => ({ value, label: value })),
                    ]}
                    onValueChange={(value) => changeTask(id, { path: value })}
                  />
                  <label>
                    {t('factory.verificationTaskName')}
                    <input
                      value={task}
                      aria-invalid={task !== '' && !isLiteralOctaTask(task)}
                      onChange={(event) => changeTask(id, { task: event.target.value })}
                    />
                  </label>
                  {task !== '' && !isLiteralOctaTask(task) ? (
                    <p role="alert">{t('factory.verificationInvalidTaskName')}</p>
                  ) : null}
                  <button
                    type="button"
                    onClick={() => setTasks(tasks.filter((item) => item.id !== id))}
                  >
                    {t('factory.verificationRemoveTask')}
                  </button>
                </fieldset>
              ))}
              <button
                type="button"
                disabled={tasks.length >= MAX_FILES}
                onClick={() =>
                  setTasks([...tasks, { id: crypto.randomUUID(), path: '', task: '' }])
                }
              >
                {t('factory.verificationAddTask')}
              </button>
            </>
          ) : null}
          {!withinLimit ? <p role="alert">{t('factory.verificationLimit')}</p> : null}
          {failed ? <p role="alert">{t('factory.verificationFailed')}</p> : null}
          <button type="button" disabled={!ready || busy} onClick={() => void freeze()}>
            {t('factory.verificationFreeze')}
          </button>
        </>
      ) : null}
    </fieldset>
  );
}
