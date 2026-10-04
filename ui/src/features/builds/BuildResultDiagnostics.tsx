import type { BuildResultDiagnosticsApi } from './api';
import { BuildArtifacts } from './BuildArtifacts';
import { BuildCacheSessions } from './BuildCacheSessions';
import { BuildResultRetention } from './BuildResultRetention';
import styles from './Builds.module.css';

interface BuildResultDiagnosticsProps {
  api: BuildResultDiagnosticsApi;
  buildId: string;
}

/** Presents bounded Build outputs, cache authority, and retention evidence. */
export function BuildResultDiagnostics({ api, buildId }: BuildResultDiagnosticsProps) {
  return (
    <div className={styles.resultDiagnostics}>
      <BuildArtifacts api={api} buildId={buildId} />
      <BuildCacheSessions api={api} buildId={buildId} />
      <BuildResultRetention api={api} buildId={buildId} />
    </div>
  );
}
