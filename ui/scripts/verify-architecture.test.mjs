import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { verifyArchitecture } from './verify-architecture.mjs';

const POLICY = {
  schemaVersion: 1,
  allowedRuntimeDependencies: [],
  allowedDevelopmentDependencies: [],
  allowedProductAssets: [],
  networkBoundaries: ['src/api/client.ts', 'src/api/readiness.ts'],
  testSupportFiles: [],
  persistenceBoundary: 'src/app/shell/preferences.ts',
  queryKeyBoundary: 'src/app/query.ts',
  designTokenStylesheet: 'src/styles/tokens.css',
  bundleBudgets: {
    entryJavaScriptBytes: 64,
    totalJavaScriptBytes: 96,
    totalCssBytes: 64,
    totalStaticBytes: 256,
    assetCount: 8,
  },
};

test('accepts the declared source and bundle boundaries', async (context) => {
  const root = await fixture(context);
  await write(root, 'dist/index.html', '<script type="module" src="./assets/index.js"></script>');
  await write(root, 'dist/assets/index.js', 'export const ready=true;');
  assert.deepEqual(await verifyArchitecture(root, { distDirectory: 'dist' }), []);
});

const sourceViolations = [
  {
    name: 'non-allowlisted dependencies',
    prepare: async (root) => {
      await writeJson(root, 'package.json', {
        private: true,
        dependencies: { 'copied-dashboard-kit': '1.0.0' },
        devDependencies: {},
      });
    },
    expected: 'non-allowlisted dependency copied-dashboard-kit',
  },
  {
    name: 'non-allowlisted imports',
    prepare: (root) => write(root, 'src/main.ts', "import 'copied-dashboard-kit';"),
    expected: 'import copied-dashboard-kit is not an allowlisted runtime dependency',
  },
  {
    name: 'non-tree-shaken icon imports',
    prepare: (root) =>
      write(root, 'src/main.ts', "import * as Icons from 'lucide-react';\nIcons.X;"),
    policy: { allowedRuntimeDependencies: ['lucide-react'] },
    manifest: { dependencies: { 'lucide-react': '1.49.0' } },
    expected: 'lucide-react icons must use named tree-shaken imports',
  },
  {
    name: 'cross-feature internals',
    prepare: (root) =>
      write(root, 'src/features/builds/view.ts', "import '../../features/audit/internal';"),
    expected: 'feature builds must not import feature audit internals',
  },
  {
    name: 'direct network calls',
    prepare: (root) =>
      write(root, 'src/main.ts', "const request = window.fetch; void request('/api/v1/projects');"),
    expected: 'direct fetch is restricted',
  },
  {
    name: 'arbitrary storage access',
    prepare: (root) => write(root, 'src/main.ts', "localStorage.setItem('domain-data', 'x');"),
    expected: 'browser persistence is restricted',
  },
  {
    name: 'analytics integrations',
    prepare: (root) =>
      write(root, 'src/main.ts', "globalThis.dataLayer.push({ event: 'opened' });"),
    expected: 'analytics and telemetry integrations are forbidden',
  },
  {
    name: 'analytics integrations in public JavaScript',
    prepare: (root) => write(root, 'public/tracker.js', "window['dataLayer'].push('opened');"),
    expected: 'analytics and telemetry integrations are forbidden',
  },
  {
    name: 'external font requests',
    prepare: (root) =>
      write(root, 'src/external.module.css', "@import url('https://fonts.example.test/font.css');"),
    expected: 'external stylesheet and font requests are forbidden',
  },
  {
    name: 'external resources in JSX',
    prepare: (root) =>
      write(
        root,
        'src/main.tsx',
        "export const Font = () => <link href='https://fonts.test/ui.css' />;",
      ),
    expected: 'external stylesheet and font requests are forbidden',
  },
  {
    name: 'external resources in the HTML entry',
    prepare: (root) =>
      write(root, 'index.html', "<link rel='stylesheet' href='https://fonts.test/ui.css'>"),
    expected: 'external stylesheet and script requests are forbidden',
  },
  {
    name: 'copied template assets',
    prepare: (root) => write(root, 'src/template-logo.svg', '<svg />'),
    expected: 'product image or font asset is not allowlisted',
  },
  {
    name: 'scattered query keys',
    prepare: (root) =>
      write(
        root,
        'src/main.ts',
        "const selected = false; const query = { queryKey: selected ? ['selected'] : ['projects'] };",
      ),
    expected: 'query-key literals must be declared',
  },
  {
    name: 'scattered timing literals',
    prepare: (root) => write(root, 'src/main.ts', 'setTimeout(() => undefined, 500);'),
    expected: 'timing literals must be named and centralized',
  },
  {
    name: 'production files disguised as test support',
    prepare: (root) =>
      write(root, 'src/TrackingTestSupport.ts', "void window.fetch('/api/v1/projects');"),
    expected: 'direct fetch is restricted',
  },
  {
    name: 'test-support imports from production',
    prepare: async (root) => {
      await write(root, 'src/ProjectTestSupport.ts', 'export const fixture = true;');
      await write(
        root,
        'src/main.ts',
        "import { fixture } from './ProjectTestSupport'; void fixture;",
      );
    },
    policy: { testSupportFiles: ['src/ProjectTestSupport.ts'] },
    expected: 'production modules must not import test support',
  },
  {
    name: 'duplicated styling constants',
    prepare: async (root) => {
      await write(root, 'src/first.module.css', '.first { color: #123456; }');
      await write(root, 'src/second.module.css', '.second { border-color: #123456; }');
    },
    expected: 'color literals must be centralized in the design tokens',
  },
  {
    name: 'browser-native selects',
    prepare: (root) => write(root, 'src/main.tsx', 'export const Native = () => <select />;'),
    expected: 'browser-native select elements are forbidden',
  },
];

for (const example of sourceViolations) {
  test(`rejects ${example.name}`, async (context) => {
    const root = await fixture(context, example.policy, example.manifest);
    await example.prepare(root);
    const violations = await verifyArchitecture(root);
    assert.ok(
      violations.some((violation) => violation.includes(example.expected)),
      violations.join('\n'),
    );
  });
}

test('rejects an oversized production entry', async (context) => {
  const root = await fixture(context);
  await write(root, 'dist/index.html', '<script type="module" src="./assets/index.js"></script>');
  await write(root, 'dist/assets/index.js', `export const payload='${'x'.repeat(100)}';`);
  const violations = await verifyArchitecture(root, { distDirectory: 'dist' });
  assert.ok(violations.some((violation) => violation.includes('entry JavaScript')));
});

async function fixture(context, policyOverrides = {}, manifestOverrides = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'octacity-console-policy-'));
  context.after(() => rm(root, { recursive: true, force: true }));
  await writeJson(root, 'architecture-policy.json', { ...POLICY, ...policyOverrides });
  await writeJson(root, 'package.json', {
    private: true,
    dependencies: {},
    devDependencies: {},
    ...manifestOverrides,
  });
  await write(root, 'src/main.ts', 'export {};');
  await write(root, 'src/api/client.ts', "export const request = () => fetch('/api/v1');");
  await write(root, 'src/api/readiness.ts', 'export {};');
  await write(root, 'src/app/query.ts', "export const queryKeys = { projects: ['projects'] };");
  await write(
    root,
    'src/app/shell/preferences.ts',
    'export const preferences = globalThis.localStorage;',
  );
  await write(root, 'src/styles/tokens.css', ':root { --color-text: #123456; }');
  return root;
}

async function writeJson(root, relativePath, value) {
  await write(root, relativePath, `${JSON.stringify(value, null, 2)}\n`);
}

async function write(root, relativePath, contents) {
  const target = path.join(root, relativePath);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, contents, 'utf8');
}
