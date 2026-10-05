import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const tokens = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');
const globalStyles = readFileSync(new URL('./global.css', import.meta.url), 'utf8');
const shellStyles = readFileSync(new URL('../app/shell/Shell.module.css', import.meta.url), 'utf8');

const darkSelector = ":root[data-resolved-theme='dark']";
const darkStart = tokens.indexOf(darkSelector);
const lightTokens = tokens.slice(0, darkStart);
const darkTokens = tokens.slice(darkStart);

const colors = parseColors(lightTokens);
const darkColors = { ...colors, ...parseColors(darkTokens) };
const CONTRAST_PAIRS = [
  ['color-text-primary', 'color-canvas'],
  ['color-text-primary', 'color-surface'],
  ['color-text-primary', 'color-surface-subtle'],
  ['color-text-muted', 'color-canvas'],
  ['color-text-muted', 'color-surface'],
  ['color-text-muted', 'color-surface-subtle'],
  ['color-brand-action', 'color-surface'],
  ['color-sidebar-text', 'color-evergreen-950'],
  ['color-sidebar-muted', 'color-evergreen-950'],
  ['color-warning-ink', 'color-surface'],
  ['color-warning-ink', 'color-warning-soft'],
  ['color-danger-ink', 'color-surface'],
  ['color-danger-ink', 'color-danger-soft'],
  ['color-information-ink', 'color-surface'],
  ['color-information-ink', 'color-information-soft'],
  ['color-success-ink', 'color-surface'],
  ['color-success-ink', 'color-brand-soft'],
  ['color-on-brand', 'color-brand-action'],
  ['color-security-ink', 'color-brand-soft'],
] as const;

function parseColors(source: string): Record<string, string> {
  return Object.fromEntries(
    [...source.matchAll(/--(?<name>color-[a-z0-9-]+):\s*(?<value>#[0-9a-f]{6});/g)].map((match) => [
      match.groups?.name,
      match.groups?.value,
    ]),
  );
}

describe('evergreen tokens', () => {
  it('keeps the original light palette and system typography centrally defined', () => {
    expect(colors).toMatchObject({
      'color-brand-accent': '#4ba786',
      'color-brand-action': '#2f765e',
      'color-brand-soft': '#dff2ea',
      'color-canvas': '#f5f7f6',
      'color-evergreen-800': '#1c3b30',
      'color-evergreen-950': '#10251d',
      'color-surface': '#ffffff',
    });
    expect(tokens).toContain('--primary-rail-width: 5.75rem');
    expect(tokens).toContain('--utility-header-height: 4rem');
    expect(tokens).toContain('-apple-system');
  });

  it('keeps reusable shell colors and dimensions in the token seam', () => {
    expect(shellStyles).not.toMatch(/#[0-9a-f]{3,8}\b|rgb\(/i);
    expect(shellStyles).toContain('var(--primary-rail-width)');
    expect(shellStyles).toContain('var(--explorer-width)');
    expect(tokens).toContain('--color-security-border: #bedbce');
    expect(tokens).toContain('--color-sidebar-hover: rgb(255 255 255 / 7%)');
  });

  it.each(CONTRAST_PAIRS)(
    '%s has WCAG AA text contrast on %s',
    (foregroundName, backgroundName) => {
      const foreground = colors[foregroundName];
      const background = colors[backgroundName];
      expect(foreground).toBeDefined();
      expect(background).toBeDefined();
      expect(contrast(foreground ?? '', background ?? '')).toBeGreaterThanOrEqual(4.5);
    },
  );

  it.each(CONTRAST_PAIRS)(
    'dark %s has WCAG AA text contrast on %s',
    (foregroundName, backgroundName) => {
      const foreground = darkColors[foregroundName];
      const background = darkColors[backgroundName];
      expect(foreground).toBeDefined();
      expect(background).toBeDefined();
      expect(contrast(foreground ?? '', background ?? '')).toBeGreaterThanOrEqual(4.5);
    },
  );

  it('keeps keyboard focus visible and suppresses motion without hiding state changes', () => {
    expect(globalStyles).toMatch(/:focus-visible\s*\{[^}]*outline:/s);
    expect(globalStyles).toMatch(/@media \(prefers-reduced-motion: reduce\)/);
    expect(globalStyles).toContain('animation-duration: 0.01ms !important');
    expect(globalStyles).toContain('animation-iteration-count: 1 !important');
    expect(globalStyles).toContain('transition-duration: 0.01ms !important');
    expect(globalStyles).toContain('scroll-behavior: auto !important');
  });
});

function contrast(first: string, second: string): number {
  const [lighter, darker] = [luminance(first), luminance(second)].sort(
    (left, right) => right - left,
  );
  return ((lighter ?? 0) + 0.05) / ((darker ?? 0) + 0.05);
}

function luminance(color: string): number {
  const channels = color
    .slice(1)
    .match(/.{2}/g)
    ?.map((channel) => Number.parseInt(channel, 16) / 255)
    .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4));
  if (channels?.length !== 3) {
    throw new Error(`Invalid color token: ${color}`);
  }
  return 0.2126 * (channels[0] ?? 0) + 0.7152 * (channels[1] ?? 0) + 0.0722 * (channels[2] ?? 0);
}
