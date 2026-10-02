import { useQuery } from '@tanstack/react-query';
import {
  Boxes,
  ChevronLeft,
  ChevronRight,
  FolderKanban,
  Menu,
  Network,
  ScrollText,
  ServerCog,
  ShieldAlert,
} from 'lucide-react';
import { useState } from 'react';
import { NavLink, Outlet } from 'react-router-dom';

import type { ReadinessProbe } from '../../api/readiness';
import { queryKeys, READINESS_REFRESH_MILLISECONDS } from '../query';
import { CONSOLE_PATHS } from '../routes';
import styles from './Shell.module.css';

const navigation = [
  { label: 'Projects', path: CONSOLE_PATHS.projects, icon: FolderKanban },
  { label: 'Agents', path: CONSOLE_PATHS.agents, icon: ServerCog },
  { label: 'Agent Pools', path: CONSOLE_PATHS.agentPools, icon: Network },
  { label: 'Audit', path: CONSOLE_PATHS.audit, icon: ScrollText },
] as const;

interface AppShellProps {
  readinessProbe: ReadinessProbe;
}

export function AppShell({ readinessProbe }: AppShellProps) {
  const [collapsed, setCollapsed] = useState(false);
  const readiness = useQuery({
    queryFn: ({ signal }) => readinessProbe(signal),
    queryKey: queryKeys.readiness,
    refetchInterval: READINESS_REFRESH_MILLISECONDS,
    refetchIntervalInBackground: false,
  });
  const readinessState = readiness.data ?? 'unreachable';
  const readinessLabel = readiness.isPending
    ? 'Checking readiness'
    : readinessLabels[readinessState];
  const readinessTone = readiness.isPending ? 'checking' : readinessState;

  return (
    <div className={collapsed ? `${styles.shell} ${styles.collapsed}` : styles.shell}>
      <a className={styles.skipLink} href="#console-content">
        Skip to content
      </a>
      <aside className={styles.sidebar} id="primary-navigation">
        <NavLink
          aria-label="OctaCity Projects"
          className={styles.brand ?? ''}
          to={CONSOLE_PATHS.projects}
        >
          <span className={styles.brandMark} aria-hidden="true">
            <Boxes size={22} strokeWidth={1.8} />
          </span>
          <span className={styles.brandText}>
            <strong>OctaCity</strong>
            <small>Operator Console</small>
          </span>
        </NavLink>

        <button
          aria-controls="primary-navigation"
          aria-expanded={!collapsed}
          className={styles.collapseButton}
          onClick={() => setCollapsed((current) => !current)}
          type="button"
        >
          {collapsed ? <ChevronRight aria-hidden="true" /> : <ChevronLeft aria-hidden="true" />}
          <span className={styles.navigationLabel}>
            {collapsed ? 'Expand navigation' : 'Collapse navigation'}
          </span>
        </button>

        <nav aria-label="Primary navigation" className={styles.navigation}>
          <p className={styles.navigationHeading}>Operations</p>
          {navigation.map(({ icon: Icon, label, path }) => (
            <NavLink
              className={({ isActive }) =>
                isActive
                  ? `${styles.navigationLink} ${styles.activeNavigationLink}`
                  : styles.navigationLink
              }
              key={path}
              to={path}
            >
              <Icon aria-hidden="true" size={19} strokeWidth={1.8} />
              <span className={styles.navigationLabel}>{label}</span>
            </NavLink>
          ))}
        </nav>
      </aside>

      <div className={styles.workspace}>
        <header className={styles.topBar}>
          <div className={styles.topBarTitle}>
            <Menu aria-hidden="true" size={18} />
            <span>Operations</span>
          </div>
          <div
            aria-live="polite"
            className={`${styles.readiness ?? ''} ${styles[`readiness_${readinessTone}`] ?? ''}`}
            role="status"
          >
            <span className={styles.readinessDot} aria-hidden="true" />
            {readinessLabel}
          </div>
        </header>

        <aside className={styles.securityBanner} aria-label="Security notice">
          <ShieldAlert aria-hidden="true" size={18} strokeWidth={1.9} />
          <p>
            <strong>Trusted network only.</strong> This console is unauthenticated and must not be
            exposed to an untrusted network.
          </p>
        </aside>

        <main className={styles.content} id="console-content" tabIndex={-1}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}

const readinessLabels = {
  ready: 'Server ready',
  unavailable: 'Server unavailable',
  unreachable: 'Server unreachable',
} as const;
