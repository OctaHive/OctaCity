import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const tokens = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');
const shellStyles = readFileSync(new URL('../app/shell/Shell.module.css', import.meta.url), 'utf8');

const colors = Object.fromEntries(
  [...tokens.matchAll(/--(?<name>color-[a-z0-9-]+):\s*(?<value>#[0-9a-f]{6});/g)].map((match) => [
    match.groups?.name,
    match.groups?.value,
  ]),
);

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
    expect(tokens).toContain('--sidebar-width: 14rem');
    expect(tokens).toContain('-apple-system');
  });

  it('keeps reusable shell colors and timing in the token seam', () => {
    expect(shellStyles).not.toMatch(/#[0-9a-f]{3,8}\b|rgb\(/i);
    expect(shellStyles).toContain('transition: width var(--motion-fast) ease');
    expect(tokens).toContain('--color-security-border: #bedbce');
    expect(tokens).toContain('--color-sidebar-hover: rgb(255 255 255 / 7%)');
  });

  it.each([
    ['color-text-primary', 'color-surface'],
    ['color-text-muted', 'color-surface'],
    ['color-brand-action', 'color-surface'],
    ['color-sidebar-text', 'color-evergreen-950'],
    ['color-sidebar-muted', 'color-evergreen-950'],
    ['color-warning-ink', 'color-surface'],
    ['color-warning-ink', 'color-warning-soft'],
    ['color-danger-ink', 'color-surface'],
    ['color-information-ink', 'color-surface'],
    ['color-success-ink', 'color-surface'],
  ])('%s has WCAG AA text contrast on %s', (foregroundName, backgroundName) => {
    const foreground = colors[foregroundName];
    const background = colors[backgroundName];
    expect(foreground).toBeDefined();
    expect(background).toBeDefined();
    expect(contrast(foreground ?? '', background ?? '')).toBeGreaterThanOrEqual(4.5);
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
