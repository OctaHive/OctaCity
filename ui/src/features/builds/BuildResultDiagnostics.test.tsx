// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { queryKeys } from '../../app/query';
import { BuildResultDiagnostics } from './BuildResultDiagnostics';
import type {
  ArtifactPage,
  BuildResultDiagnosticsApi,
  BuildResultRetentionResource,
  CacheSessionPage,
} from './api';

const BUILD_ID = '11111111-1111-4111-8111-111111111111';
const ARTIFACT_ID = '22222222-2222-4222-8222-222222222222';
const PRIVATE_URL = 'https://objects.example.invalid/download?X-Amz-Credential=private-capability';

afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('Build Result diagnostics', () => {
  it('lists published outputs and launches a fresh download without retaining its capability', async () => {
    const downloadWindow = fakeDownloadWindow();
    const open = vi.spyOn(window, 'open').mockReturnValue(downloadWindow.window);
    const consoleLog = vi.spyOn(console, 'log').mockImplementation(() => undefined);
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const api = diagnosticsApi({ artifacts: artifactPage() });
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Published Artifacts' });
    expect(await within(panel).findByText('coverage.xml')).toBeTruthy();
    expect(within(panel).getByText('Cobertura report')).toBeTruthy();
    expect(api.authorizeArtifactDownload).not.toHaveBeenCalled();

    await userEvent.click(within(panel).getByRole('button', { name: 'Download coverage.xml' }));

    expect(open).toHaveBeenCalledWith('about:blank', '_blank', 'popup');
    await waitFor(() => expect(downloadWindow.replace).toHaveBeenCalledWith(PRIVATE_URL));
    expect(downloadWindow.window.opener).toBeNull();
    expect(
      (downloadWindow.document.querySelector('meta[name="referrer"]') as HTMLMetaElement | null)
        ?.content,
    ).toBe('no-referrer');
    expect(api.authorizeArtifactDownload).toHaveBeenCalledWith(
      ARTIFACT_ID,
      expect.any(AbortSignal),
    );
    expect(document.documentElement.innerHTML).not.toContain(PRIVATE_URL);
    expect(localStorage).toHaveLength(0);
    expect(sessionStorage).toHaveLength(0);
    expect(consoleLog).not.toHaveBeenCalled();
    expect(consoleWarn).not.toHaveBeenCalled();
  });

  it('renders only the secret-free cache diagnostic projection', async () => {
    const secret = 'cache-bearer-must-not-appear';
    const cache = {
      ...cachePage().items[0],
      credential: secret,
      physical_location: `s3://private/${secret}`,
    } as CacheSessionPage['items'][number];
    renderDiagnostics(diagnosticsApi({ cacheSessions: { items: [cache] } }));

    const panel = await screen.findByRole('region', { name: 'Cache sessions' });
    expect(await within(panel).findByText('compiler-cache')).toBeTruthy();
    expect(within(panel).getByText('Read and write')).toBeTruthy();
    expect(within(panel).getByText('Active')).toBeTruthy();
    expect(document.documentElement.textContent).not.toContain(secret);
    expect(document.documentElement.textContent).not.toContain('s3://private');
  });

  it.each([
    ['absent', null, 'No hold'],
    ['held', hold('active'), 'Active hold'],
    ['expired', hold('expired'), 'Expired hold'],
  ] as const)('distinguishes an %s retention hold', async (_case, holdResource, label) => {
    renderDiagnostics(
      diagnosticsApi({
        retention: {
          ...retentionResource(),
          hold: holdResource,
        },
      }),
    );

    const panel = await screen.findByRole('region', { name: 'Build Result retention' });
    expect(await within(panel).findByText(label)).toBeTruthy();
    expect(within(panel).getAllByText('Visible', { selector: 'dd' })).toHaveLength(3);
    expect(within(panel).getByText('Unavailable', { selector: 'dd' })).toBeTruthy();
  });

  it('validates and places a time-bounded hold with audit correlation', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '88888888-8888-4888-8888-888888888888'),
    });
    const active = { ...retentionResource(), hold: hold('active') };
    const api = diagnosticsApi();
    vi.mocked(api.getBuildResultRetention)
      .mockReset()
      .mockResolvedValueOnce(retentionResource())
      .mockResolvedValue(active);
    vi.mocked(api.placeBuildResultHold).mockResolvedValue({
      disposition: 'applied',
      retention: active,
    });
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Build Result retention' });
    await userEvent.click(await within(panel).findByRole('button', { name: 'Review hold' }));
    expect(await within(panel).findByText('A hold reason is required.')).toBeTruthy();

    const reason = within(panel).getByRole('textbox', { name: 'Hold reason' });
    await userEvent.type(reason, '💚'.repeat(129));
    await userEvent.click(within(panel).getByRole('button', { name: 'Review hold' }));
    expect(
      await within(panel).findByText('The hold reason cannot exceed 512 UTF-8 bytes.'),
    ).toBeTruthy();

    await userEvent.clear(reason);
    await userEvent.type(reason, 'incident evidence');
    await userEvent.selectOptions(
      within(panel).getByRole('combobox', { name: 'Hold duration' }),
      'time_bounded',
    );
    const expiry = within(panel).getByLabelText('Hold expiry');
    await userEvent.type(expiry, '2000-01-02T03:04');
    await userEvent.click(within(panel).getByRole('button', { name: 'Review hold' }));
    expect(
      await within(panel).findByText('Hold expiry must be a valid future date and time.'),
    ).toBeTruthy();

    await userEvent.clear(expiry);
    await userEvent.type(expiry, '2099-01-02T03:04');
    await userEvent.click(within(panel).getByRole('button', { name: 'Review hold' }));

    const dialog = screen.getByRole('dialog', { name: `Place hold on Build ${BUILD_ID}?` });
    expect(
      within(dialog).getByText(/protects metadata, logs, Artifacts, and reports/),
    ).toBeTruthy();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Place hold' }));

    expect(api.placeBuildResultHold).toHaveBeenCalledWith(
      BUILD_ID,
      {
        expires_at_unix_ms: new Date('2099-01-02T03:04').getTime(),
        reason: 'incident evidence',
      },
      { 'Idempotency-Key': '88888888-8888-4888-8888-888888888888' },
    );
    expect(await within(dialog).findByText('Retention hold placed (Applied).')).toBeTruthy();
    expect(
      within(dialog)
        .getByRole('link', { name: 'View placement audit evidence' })
        .getAttribute('href'),
    ).toBe('/audit?request_identity=request-1');
    await waitFor(() => expect(api.getBuildResultRetention).toHaveBeenCalledTimes(2));
  });

  it('releases the current hold version and refreshes after a stale-version rejection', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '99999999-9999-4999-8999-999999999999'),
    });
    const api = diagnosticsApi({
      retention: { ...retentionResource(), hold: hold('active') },
    });
    vi.mocked(api.getBuildResultRetention)
      .mockResolvedValueOnce({ ...retentionResource(), hold: hold('active') })
      .mockResolvedValue({ ...retentionResource(), hold: { ...hold('active'), version: 3 } });
    vi.mocked(api.releaseBuildResultHold).mockRejectedValue(
      new ManagementApiError({
        code: 'precondition_failed',
        message: 'The retention hold changed.',
        requestId: 'request-stale',
        retryAfterMilliseconds: null,
        status: 412,
      }),
    );
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Build Result retention' });
    await userEvent.click(await within(panel).findByRole('button', { name: 'Release hold' }));
    const dialog = screen.getByRole('dialog', { name: `Release hold on Build ${BUILD_ID}?` });
    expect(within(dialog).getByText(/eligible for deletion/)).toBeTruthy();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Release hold' }));

    expect(api.releaseBuildResultHold).toHaveBeenCalledWith(BUILD_ID, {
      'Idempotency-Key': '99999999-9999-4999-8999-999999999999',
      'If-Match': '"2"',
    });
    expect(await within(dialog).findByText('The retention hold changed.')).toBeTruthy();
    expect(within(dialog).getByText('Request ID: request-stale')).toBeTruthy();
    expect(within(dialog).queryByRole('button', { name: 'Retry same command' })).toBeNull();

    await userEvent.click(within(dialog).getByRole('button', { name: 'Refresh current data' }));
    await waitFor(() => expect(api.getBuildResultRetention).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(await within(panel).findByText('3', { selector: 'dd' })).toBeTruthy();
    expect(api.releaseBuildResultHold).toHaveBeenCalledTimes(1);
  });

  it('refuses a non-HTTP download capability', async () => {
    const downloadWindow = fakeDownloadWindow();
    const open = vi.spyOn(window, 'open').mockReturnValue(downloadWindow.window);
    const api = diagnosticsApi({ artifacts: artifactPage() });
    vi.mocked(api.authorizeArtifactDownload).mockResolvedValue({
      artifact: artifactPage().items[0]!,
      expires_at_unix_ms: 1_700_000_100_000,
      get_url: 'javascript:alert(document.domain)',
    });
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Published Artifacts' });
    await userEvent.click(
      await within(panel).findByRole('button', { name: 'Download coverage.xml' }),
    );

    expect((await within(panel).findByRole('alert')).textContent).toBe(
      'The download could not be started.',
    );
    expect(open).toHaveBeenCalledWith('about:blank', '_blank', 'popup');
    expect(downloadWindow.close).toHaveBeenCalledOnce();
    expect(downloadWindow.replace).not.toHaveBeenCalled();
  });

  it('does not request a private capability when the browser blocks the download window', async () => {
    vi.spyOn(window, 'open').mockReturnValue(null);
    const api = diagnosticsApi({ artifacts: artifactPage() });
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Published Artifacts' });
    await userEvent.click(
      await within(panel).findByRole('button', { name: 'Download coverage.xml' }),
    );

    expect((await within(panel).findByRole('alert')).textContent).toContain(
      'The browser blocked the download window.',
    );
    expect(api.authorizeArtifactDownload).not.toHaveBeenCalled();
  });

  it('offers a bounded retry after an initial diagnostic failure', async () => {
    const api = diagnosticsApi();
    vi.mocked(api.listBuildArtifacts)
      .mockRejectedValueOnce(new Error('temporary failure'))
      .mockResolvedValueOnce(artifactPage());
    renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Published Artifacts' });
    await userEvent.click(await within(panel).findByRole('button', { name: 'Retry' }));

    expect(await within(panel).findByText('coverage.xml')).toBeTruthy();
    expect(api.listBuildArtifacts).toHaveBeenCalledTimes(2);
  });

  it('retains and marks cached diagnostics when a refresh fails', async () => {
    const api = diagnosticsApi({ artifacts: artifactPage() });
    vi.mocked(api.listBuildArtifacts)
      .mockReset()
      .mockResolvedValueOnce(artifactPage())
      .mockRejectedValueOnce(new Error('refresh failed'));
    const queryClient = renderDiagnostics(api);

    const panel = await screen.findByRole('region', { name: 'Published Artifacts' });
    expect(await within(panel).findByText('coverage.xml')).toBeTruthy();
    await act(() =>
      queryClient.invalidateQueries({ queryKey: queryKeys.buildArtifacts(BUILD_ID) }),
    );

    expect(
      await within(panel).findByText(
        'Refresh failed. Showing the last loaded published outputs data.',
      ),
    ).toBeTruthy();
    expect(within(panel).getByText('coverage.xml')).toBeTruthy();
  });
});

function fakeDownloadWindow() {
  const popupDocument = document.implementation.createHTMLDocument();
  const close = vi.fn();
  const replace = vi.fn();
  const windowLike = {
    close,
    document: popupDocument,
    location: { replace },
    opener: window,
  } as unknown as Window;
  return { close, document: popupDocument, replace, window: windowLike };
}

function renderDiagnostics(api: BuildResultDiagnosticsApi) {
  const queryClient = createConsoleQueryClient();
  render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <BuildResultDiagnostics api={api} buildId={BUILD_ID} />
      </QueryClientProvider>
    </MemoryRouter>,
  );
  return queryClient;
}

function diagnosticsApi({
  artifacts = { items: [] },
  cacheSessions = { items: [] },
  retention = retentionResource(),
}: {
  artifacts?: ArtifactPage;
  cacheSessions?: CacheSessionPage;
  retention?: BuildResultRetentionResource;
} = {}): BuildResultDiagnosticsApi {
  return {
    authorizeArtifactDownload: vi.fn().mockResolvedValue({
      artifact: artifactPage().items[0]!,
      expires_at_unix_ms: 1_700_000_100_000,
      get_url: PRIVATE_URL,
    }),
    getBuildResultRetention: vi.fn().mockResolvedValue(retention),
    listBuildArtifacts: vi.fn().mockResolvedValue(artifacts),
    listBuildCacheSessions: vi.fn().mockResolvedValue(cacheSessions),
    placeBuildResultHold: vi.fn(),
    releaseBuildResultHold: vi.fn(),
  };
}

function artifactPage(): ArtifactPage {
  return {
    items: [
      {
        attempt_id: 'attempt-1',
        build_id: BUILD_ID,
        id: ARTIFACT_ID,
        job_id: 'job-1',
        media_type: 'application/xml',
        name: 'coverage.xml',
        output_type: { format: 'cobertura', kind: 'report' },
        published_at_unix_ms: 1_700_000_000_000,
        sha256: 'a'.repeat(64),
        size_bytes: 2_048,
      },
    ],
  };
}

function cachePage(): CacheSessionPage {
  return {
    items: [
      {
        agent_id: 'agent-1',
        build_id: BUILD_ID,
        created_at_unix_ms: 1_700_000_000_000,
        expires_at_unix_ms: 1_700_000_200_000,
        id: 'cache-session-1',
        job_id: 'job-1',
        lease_id: 'lease-1',
        namespace: 'compiler-cache',
        project_id: 'project-1',
        quota_bytes: 1_048_576,
        read: true,
        registration_epoch: 3,
        retention_until_unix_ms: 1_700_000_300_000,
        revoked_at_unix_ms: null,
        state: 'active',
        write: true,
      },
    ],
  };
}

function retentionResource(): BuildResultRetentionResource {
  return {
    build_id: BUILD_ID,
    deadlines: {
      artifacts_at_unix_ms: 1_700_000_400_000,
      logs_at_unix_ms: 1_700_000_300_000,
      metadata_at_unix_ms: 1_700_000_200_000,
      reports_at_unix_ms: 1_700_000_500_000,
    },
    hold: null,
    visibility: { artifacts: true, logs: false, metadata: true, reports: true },
  };
}

function hold(state: 'active' | 'expired'): NonNullable<BuildResultRetentionResource['hold']> {
  return {
    created_at_unix_ms: 1_700_000_000_000,
    creation_audit: {
      actor_identity: null,
      actor_kind: 'unauthenticated_management',
      request_identity: 'request-1',
    },
    expires_at_unix_ms: 1_700_000_100_000,
    reason: 'incident evidence',
    release_audit: null,
    released_at_unix_ms: null,
    state,
    version: 2,
  };
}
