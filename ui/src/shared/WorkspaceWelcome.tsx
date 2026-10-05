import type { ReactNode } from 'react';

import styles from './WorkspaceWelcome.module.css';

export type WorkspaceWelcomeKind = 'agents' | 'builds' | 'projects';

interface WorkspaceWelcomeProps {
  description: string;
  eyebrow: string;
  illustrationLabel: string;
  kind: WorkspaceWelcomeKind;
  title: string;
}

/** Fills an unselected workspace with section-specific orientation and illustration. */
export function WorkspaceWelcome({
  description,
  eyebrow,
  illustrationLabel,
  kind,
  title,
}: WorkspaceWelcomeProps) {
  return (
    <section
      aria-labelledby="page-title"
      className={styles.welcome}
      data-kind={kind}
      data-workspace-welcome="true"
    >
      <div className={styles.inner}>
        <WorkspaceIllustration kind={kind} label={illustrationLabel} />
        <header className={styles.copy}>
          <p className={styles.eyebrow}>{eyebrow}</p>
          <h1 id="page-title">{title}</h1>
          <p className={styles.description}>{description}</p>
        </header>
      </div>
    </section>
  );
}

function WorkspaceIllustration({ kind, label }: { kind: WorkspaceWelcomeKind; label: string }) {
  const scene = {
    agents: <AgentsScene />,
    builds: <BuildsScene />,
    projects: <ProjectsScene />,
  }[kind];
  return (
    <svg
      aria-label={label}
      className={styles.illustration}
      focusable="false"
      role="img"
      viewBox="0 0 720 360"
    >
      <title>{label}</title>
      <circle className={styles.halo} cx="360" cy="174" r="150" />
      <path className={styles.orbit} d="M90 222c90-154 450-194 542-23" />
      <path className={styles.orbitSoft} d="M118 264c143 70 393 60 492-70" />
      {scene}
      <g className={styles.sparkles}>
        <circle cx="91" cy="126" r="5" />
        <circle cx="622" cy="103" r="7" />
        <circle cx="654" cy="236" r="4" />
        <path d="m131 83 5 10 10 5-10 5-5 10-5-10-10-5 10-5Z" />
        <path d="m589 276 4 8 8 4-8 4-4 8-4-8-8-4 8-4Z" />
      </g>
    </svg>
  );
}

function Window({ children }: { children: ReactNode }) {
  return (
    <g>
      <rect className={styles.windowShadow} height="224" rx="26" width="430" x="145" y="58" />
      <rect className={styles.window} height="224" rx="26" width="430" x="145" y="50" />
      <path className={styles.windowBar} d="M145 84h430" />
      <circle className={styles.windowDot} cx="170" cy="68" r="5" />
      <circle className={styles.windowDotSoft} cx="190" cy="68" r="5" />
      <circle className={styles.windowDotSoft} cx="210" cy="68" r="5" />
      {children}
    </g>
  );
}

function ProjectsScene() {
  return (
    <Window>
      <rect className={styles.sidebar} height="166" rx="12" width="92" x="165" y="96" />
      <rect className={styles.sidebarActive} height="28" rx="8" width="68" x="177" y="112" />
      <path className={styles.sidebarLine} d="M180 158h54M180 178h45M180 198h58M180 218h38" />
      <g className={styles.connectionLines}>
        <path d="M326 143v34m0-17h72m-72 17h-31m103-17v17m0-17h62v17" />
      </g>
      <ProjectCard width={68} x={292} y={106} />
      <ProjectCard width={74} x={272} y={181} />
      <ProjectCard width={72} x={364} y={181} />
      <ProjectCard width={76} x={454} y={181} />
      <g className={styles.folder} transform="translate(311 120)">
        <path d="M0 5a5 5 0 0 1 5-5h13l7 7h24a5 5 0 0 1 5 5v23a5 5 0 0 1-5 5H5a5 5 0 0 1-5-5Z" />
        <path d="M0 13h54" />
      </g>
    </Window>
  );
}

function ProjectCard({ width, x, y }: { width: number; x: number; y: number }) {
  return (
    <g>
      <rect className={styles.nodeCard} height="54" rx="12" width={width} x={x} y={y} />
      <circle className={styles.nodeMark} cx={x + 15} cy={y + 17} r="5" />
      <path
        className={styles.nodeLine}
        d={`M${x + 27} ${y + 17}h${width - 38}M${x + 14} ${y + 34}h${width - 28}`}
      />
    </g>
  );
}

function BuildsScene() {
  return (
    <Window>
      <path className={styles.pipelineLine} d="M228 180h70m123 0h70" />
      <PipelineCard state="done" x={170} />
      <PipelineCard state="running" x={306} />
      <PipelineCard state="queued" x={442} />
      <g className={styles.console}>
        <rect height="45" rx="10" width="330" x="195" y="215" />
        <path d="m218 234 10 8-10 8m24 0h35" />
        <circle cx="493" cy="237" r="5" />
      </g>
      <g className={styles.buildBadge}>
        <circle cx="360" cy="111" r="27" />
        <path d="m350 111 7 7 14-16" />
      </g>
    </Window>
  );
}

function PipelineCard({ state, x }: { state: 'done' | 'queued' | 'running'; x: number }) {
  return (
    <g>
      <rect className={styles.pipelineCard} height="62" rx="14" width="108" x={x} y="150" />
      <circle className={styles[state]} cx={x + 24} cy="174" r="9" />
      <path className={styles.nodeLine} d={`M${x + 42} 171h44M${x + 42} 188h30`} />
    </g>
  );
}

function AgentsScene() {
  return (
    <Window>
      <g className={styles.networkLines}>
        <path d="M360 122 249 180m111-58 111 58M249 180l111 66 111-66" />
      </g>
      <AgentNode kind="controller" x={360} y={122} />
      <AgentNode kind="online" x={249} y={180} />
      <AgentNode kind="online" x={471} y={180} />
      <g className={styles.serverRack} transform="translate(317 204)">
        <rect height="53" rx="12" width="86" />
        <path d="M0 26h86" />
        <circle cx="17" cy="14" r="4" />
        <circle cx="17" cy="39" r="4" />
        <path d="M31 14h36M31 39h36" />
      </g>
      <g className={styles.capacityBars}>
        <rect height="8" rx="4" width="62" x="179" y="231" />
        <rect height="8" rx="4" width="42" x="479" y="231" />
        <rect height="8" rx="4" width="27" x="179" y="247" />
        <rect height="8" rx="4" width="58" x="479" y="247" />
      </g>
    </Window>
  );
}

function AgentNode({ kind, x, y }: { kind: 'controller' | 'online'; x: number; y: number }) {
  return (
    <g>
      <circle className={styles.agentNode} cx={x} cy={y} r={kind === 'controller' ? 34 : 29} />
      <rect
        className={kind === 'controller' ? styles.agentCore : styles.agentCoreSoft}
        height={kind === 'controller' ? 24 : 20}
        rx="6"
        width={kind === 'controller' ? 34 : 30}
        x={x - (kind === 'controller' ? 17 : 15)}
        y={y - (kind === 'controller' ? 12 : 10)}
      />
      <circle className={styles.agentStatus} cx={x + 20} cy={y - 21} r="6" />
    </g>
  );
}
