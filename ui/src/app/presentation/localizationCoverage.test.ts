import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

const SOURCE_ROOT = resolve('src');
const ALLOWED_VISIBLE_LITERALS = new Set(['Ctrl/⌘ K', 'Esc', 'OctaCity', 'stderr', 'stdout']);
const LOCALIZED_ATTRIBUTES =
  /\b(?:aria-label|confirmLabel|consequence|description|label|placeholder|title)="(?<value>[^"]*[A-Za-zА-Яа-я][^"]*)"/gu;
const VISIBLE_JSX_TEXT =
  /<(?<tag>button|caption|dd|dt|h[1-6]|kbd|label|legend|li|option|p|small|span|strong|td|th)\b[^>]*>\s*(?<value>[\p{L}\p{N}][\p{L}\p{N}\s.,!?/'’:⌘-]*)\s*<\/\k<tag>>/gu;

describe('operator localization coverage', () => {
  it('keeps operator-visible literals behind the typed message catalog', () => {
    const violations = productionTsxFiles(SOURCE_ROOT).flatMap((path) =>
      visibleLiteralViolations(readFileSync(path, 'utf8')).map(
        (value) => `${relative(SOURCE_ROOT, path)}: ${value}`,
      ),
    );

    expect(violations).toEqual([]);
  });

  it('rejects representative hard-coded text and accessible labels', () => {
    expect(visibleLiteralViolations('<h1>Hard-coded heading</h1>')).toEqual(['Hard-coded heading']);
    expect(visibleLiteralViolations('<button aria-label="Hard-coded action" />')).toEqual([
      'Hard-coded action',
    ]);
  });
});

function productionTsxFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return productionTsxFiles(path);
    return entry.name.endsWith('.tsx') && !entry.name.includes('.test.') ? [path] : [];
  });
}

function visibleLiteralViolations(source: string): string[] {
  return [...matches(source, VISIBLE_JSX_TEXT), ...matches(source, LOCALIZED_ATTRIBUTES)].filter(
    (value) => !isAllowedLiteral(value),
  );
}

function matches(source: string, pattern: RegExp): string[] {
  return [...source.matchAll(pattern)]
    .map((match) => match.groups?.value?.trim() ?? '')
    .filter(Boolean);
}

function isAllowedLiteral(value: string): boolean {
  return ALLOWED_VISIBLE_LITERALS.has(value) || /^[a-z]+(?:\.[a-zA-Z]+)+$/u.test(value);
}
