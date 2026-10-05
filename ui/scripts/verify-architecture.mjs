import console from 'node:console';
import { readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import ts from 'typescript';

const SCRIPT_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIRECTORY, '..');
const SOURCE_EXTENSIONS = new Set(['.css', '.js', '.jsx', '.ts', '.tsx']);
const PRODUCT_ASSET_EXTENSIONS = new Set([
  '.gif',
  '.ico',
  '.jpeg',
  '.jpg',
  '.otf',
  '.png',
  '.svg',
  '.ttf',
  '.webp',
  '.woff',
  '.woff2',
]);
const EXACT_VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/** Verify the console's dependency, source, and optional built-output boundaries. */
export async function verifyArchitecture(root, { distDirectory = null } = {}) {
  const policy = await readJson(path.join(root, 'architecture-policy.json'));
  const manifest = await readJson(path.join(root, 'package.json'));
  const violations = [];

  verifyPolicy(policy, violations);
  verifyDependencies(manifest, policy, violations);
  await verifySources(root, policy, violations);
  if (distDirectory !== null) {
    await verifyBundle(path.resolve(root, distDirectory), policy.bundleBudgets, violations);
  }
  return violations.sort();
}

function verifyPolicy(policy, violations) {
  if (policy.schemaVersion !== 1) {
    violations.push('architecture-policy.json: schemaVersion must be 1');
  }
  for (const field of [
    'allowedRuntimeDependencies',
    'allowedDevelopmentDependencies',
    'allowedProductAssets',
    'networkBoundaries',
    'testSupportFiles',
  ]) {
    if (
      !Array.isArray(policy[field]) ||
      !policy[field].every((value) => typeof value === 'string')
    ) {
      violations.push(`architecture-policy.json: ${field} must be a string array`);
    }
  }
  for (const field of ['persistenceBoundary', 'queryKeyBoundary', 'designTokenStylesheet']) {
    if (typeof policy[field] !== 'string') {
      violations.push(`architecture-policy.json: ${field} must be a path`);
    }
  }
  const budgets = policy.bundleBudgets;
  for (const field of [
    'entryJavaScriptBytes',
    'totalJavaScriptBytes',
    'totalCssBytes',
    'totalStaticBytes',
    'assetCount',
  ]) {
    if (!Number.isSafeInteger(budgets?.[field]) || budgets[field] <= 0) {
      violations.push(`architecture-policy.json: bundleBudgets.${field} must be positive`);
    }
  }
}

function verifyDependencies(manifest, policy, violations) {
  verifyDependencyGroup(
    'dependencies',
    manifest.dependencies,
    policy.allowedRuntimeDependencies,
    violations,
  );
  verifyDependencyGroup(
    'devDependencies',
    manifest.devDependencies,
    policy.allowedDevelopmentDependencies,
    violations,
  );
}

function verifyDependencyGroup(label, dependencies = {}, allowed = [], violations) {
  const actualNames = Object.keys(dependencies).sort();
  const allowedNames = [...allowed].sort();
  for (const name of actualNames.filter((dependency) => !allowedNames.includes(dependency))) {
    violations.push(`package.json: ${label} contains non-allowlisted dependency ${name}`);
  }
  for (const name of allowedNames.filter((dependency) => !actualNames.includes(dependency))) {
    violations.push(`package.json: ${label} allowlists unused dependency ${name}`);
  }
  for (const [name, version] of Object.entries(dependencies)) {
    if (typeof version !== 'string' || !EXACT_VERSION.test(version)) {
      violations.push(`package.json: ${label}.${name} must use an exact semantic version`);
    }
  }
}

async function verifySources(root, policy, violations) {
  await verifySourceTree(root, path.join(root, 'src'), policy, violations);
  await verifySourceTree(root, path.join(root, 'public'), policy, violations, {
    missingIsEmpty: true,
  });

  const indexPath = path.join(root, 'index.html');
  try {
    verifyHtml('index.html', await readFile(indexPath, 'utf8'), violations);
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error;
  }
}

async function verifySourceTree(
  root,
  sourceRoot,
  policy,
  violations,
  { missingIsEmpty = false } = {},
) {
  for (const absolutePath of await walk(sourceRoot, { missingIsEmpty })) {
    const relativePath = normalizePath(path.relative(root, absolutePath));
    const extension = path.extname(relativePath).toLowerCase();
    if (
      PRODUCT_ASSET_EXTENSIONS.has(extension) &&
      !policy.allowedProductAssets.includes(relativePath)
    ) {
      violations.push(`${relativePath}: product image or font asset is not allowlisted`);
      continue;
    }
    if (
      (!SOURCE_EXTENSIONS.has(extension) && extension !== '.html') ||
      isTestFile(relativePath, policy)
    ) {
      continue;
    }
    const contents = await readFile(absolutePath, 'utf8');
    if (extension === '.css') {
      verifyStylesheet(relativePath, contents, policy, violations);
    } else if (extension === '.html') {
      verifyHtml(relativePath, contents, violations);
    } else {
      verifyModule(relativePath, contents, policy, violations);
    }
  }
}

function verifyModule(relativePath, contents, policy, violations) {
  const source = ts.createSourceFile(
    relativePath,
    contents,
    ts.ScriptTarget.Latest,
    true,
    scriptKind(relativePath),
  );
  for (const { named, specifier } of moduleImports(source)) {
    if (
      !specifier.startsWith('.') &&
      !policy.allowedRuntimeDependencies.includes(packageName(specifier))
    ) {
      violations.push(
        `${relativePath}: import ${specifier} is not an allowlisted runtime dependency`,
      );
    }
    if (specifier === 'lucide-react' && !named) {
      violations.push(`${relativePath}: lucide-react icons must use named tree-shaken imports`);
    }
    const sourceFeature = featureName(relativePath);
    const targetFeature = importedFeature(relativePath, specifier);
    if (sourceFeature !== null && targetFeature !== null && sourceFeature !== targetFeature) {
      violations.push(
        `${relativePath}: feature ${sourceFeature} must not import feature ${targetFeature} internals`,
      );
    }
    if (specifier.includes('TestSupport')) {
      violations.push(`${relativePath}: production modules must not import test support`);
    }
  }

  const found = new Set();
  visit(source, (node) => {
    const referencedName = globalReferenceName(node);
    if (referencedName === 'fetch' && !policy.networkBoundaries.includes(relativePath)) {
      found.add('direct fetch is restricted to declared network boundaries');
    }
    if (['XMLHttpRequest', 'WebSocket', 'EventSource', 'sendBeacon'].includes(referencedName)) {
      found.add('direct browser network primitives are forbidden');
    }
    if (['localStorage', 'sessionStorage', 'indexedDB', 'caches'].includes(referencedName)) {
      if (relativePath !== policy.persistenceBoundary || referencedName !== 'localStorage') {
        found.add('browser persistence is restricted to the preference adapter');
      }
    }
    if (['dataLayer', 'gtag', 'mixpanel', 'posthog', 'Sentry'].includes(referencedName)) {
      found.add('analytics and telemetry integrations are forbidden');
    }
    if (
      ts.isPropertyAssignment(node) &&
      propertyName(node.name) === 'queryKey' &&
      containsArrayLiteral(node.initializer) &&
      relativePath !== policy.queryKeyBoundary
    ) {
      found.add(`query-key literals must be declared in ${policy.queryKeyBoundary}`);
    }
    if (hasTimingLiteral(node)) {
      found.add('timing literals must be named and centralized');
    }
    if (isExternalJsxResource(node)) {
      found.add('external stylesheet and font requests are forbidden');
    }
  });
  for (const message of found) violations.push(`${relativePath}: ${message}`);
}

function verifyHtml(relativePath, contents, violations) {
  if (/<(?:link|script)\b[^>]+(?:href|src)=["'](?:https?:)?\/\//iu.test(contents)) {
    violations.push(`${relativePath}: external stylesheet and script requests are forbidden`);
  }
}

function verifyStylesheet(relativePath, contents, policy, violations) {
  if (/(?:https?:)?\/\//iu.test(contents)) {
    violations.push(`${relativePath}: external stylesheet and font requests are forbidden`);
  }
  if (/@font-face|\.(?:eot|otf|ttf|woff2?)(?:[?#)'"]|$)/iu.test(contents)) {
    violations.push(`${relativePath}: bundled fonts and icon fonts are forbidden`);
  }
  if (/\b(?:Font Awesome|Material Icons|Glyphicons|IcoMoon)\b/iu.test(contents)) {
    violations.push(`${relativePath}: icon-font families are forbidden`);
  }
  if (
    relativePath !== policy.designTokenStylesheet &&
    /#[\da-f]{3,8}\b|\b(?:rgb|hsl)a?\s*\(/iu.test(contents)
  ) {
    violations.push(`${relativePath}: color literals must be centralized in the design tokens`);
  }
}

async function verifyBundle(distRoot, budgets, violations) {
  let files;
  try {
    files = await walk(distRoot);
  } catch (error) {
    if (error?.code === 'ENOENT') {
      violations.push(`${normalizePath(distRoot)}: production bundle is missing`);
      return;
    }
    throw error;
  }
  const indexPath = path.join(distRoot, 'index.html');
  let index;
  try {
    index = await readFile(indexPath, 'utf8');
  } catch {
    violations.push(`${normalizePath(indexPath)}: production index.html is missing`);
    return;
  }
  const sizes = await Promise.all(
    files.map(async (file) => ({ file, size: (await stat(file)).size })),
  );
  const javascript = sizes.filter(({ file }) => path.extname(file) === '.js');
  const css = sizes.filter(({ file }) => path.extname(file) === '.css');
  const entryMatch = index.match(/<script[^>]+src=["']([^"']+\.js)["']/iu);
  const entryName = entryMatch?.[1]?.replace(/^\.?\//u, '') ?? null;
  const entry =
    entryName === null
      ? null
      : sizes.find(({ file }) => normalizePath(path.relative(distRoot, file)) === entryName);
  if (entry === null || entry === undefined) {
    violations.push('dist/index.html: production JavaScript entry could not be resolved');
  } else if (entry.size > budgets.entryJavaScriptBytes) {
    violations.push(
      bundleBudgetMessage('entry JavaScript', entry.size, budgets.entryJavaScriptBytes),
    );
  }
  verifySizeBudget(
    'total JavaScript',
    totalSize(javascript),
    budgets.totalJavaScriptBytes,
    violations,
  );
  verifySizeBudget('total CSS', totalSize(css), budgets.totalCssBytes, violations);
  verifySizeBudget('total static output', totalSize(sizes), budgets.totalStaticBytes, violations);
  if (sizes.length > budgets.assetCount) {
    violations.push(
      `bundle budget exceeded for asset count: ${sizes.length} > ${budgets.assetCount}`,
    );
  }
  for (const { file } of sizes) {
    if (path.extname(file) === '.map') {
      violations.push(`${normalizePath(path.relative(distRoot, file))}: source maps are forbidden`);
    }
  }
  const cssContents = await Promise.all(css.map(({ file }) => readFile(file, 'utf8')));
  if (/(?:https?:)?\/\//iu.test([index, ...cssContents].join('\n'))) {
    violations.push('production bundle contains an external URL request');
  }
}

function verifySizeBudget(label, actual, budget, violations) {
  if (actual > budget) violations.push(bundleBudgetMessage(label, actual, budget));
}

function bundleBudgetMessage(label, actual, budget) {
  return `bundle budget exceeded for ${label}: ${actual} bytes > ${budget} bytes`;
}

function totalSize(entries) {
  return entries.reduce((total, entry) => total + entry.size, 0);
}

function moduleImports(source) {
  const found = [];
  visit(source, (node) => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      found.push({
        named:
          node.importClause?.namedBindings !== undefined &&
          ts.isNamedImports(node.importClause.namedBindings),
        specifier: node.moduleSpecifier.text,
      });
    } else if (
      ts.isExportDeclaration(node) &&
      node.moduleSpecifier !== undefined &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      found.push({
        named: node.exportClause !== undefined && ts.isNamedExports(node.exportClause),
        specifier: node.moduleSpecifier.text,
      });
    } else if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword &&
      node.arguments.length === 1 &&
      node.arguments[0] !== undefined &&
      ts.isStringLiteral(node.arguments[0])
    ) {
      found.push({ named: false, specifier: node.arguments[0].text });
    }
  });
  return found;
}

function visit(node, callback) {
  callback(node);
  ts.forEachChild(node, (child) => visit(child, callback));
}

function scriptKind(relativePath) {
  if (relativePath.endsWith('.tsx')) return ts.ScriptKind.TSX;
  if (relativePath.endsWith('.jsx')) return ts.ScriptKind.JSX;
  if (relativePath.endsWith('.js')) return ts.ScriptKind.JS;
  return ts.ScriptKind.TS;
}

function globalReferenceName(node) {
  if (ts.isIdentifier(node)) {
    if (ts.isPropertyAccessExpression(node.parent) && node.parent.name === node) return null;
    return node.text;
  }
  if (ts.isPropertyAccessExpression(node)) return node.name.text;
  if (
    ts.isElementAccessExpression(node) &&
    node.argumentExpression !== undefined &&
    ts.isStringLiteral(node.argumentExpression)
  ) {
    return node.argumentExpression.text;
  }
  return null;
}

function propertyName(name) {
  return ts.isIdentifier(name) || ts.isStringLiteral(name) ? name.text : null;
}

function containsArrayLiteral(node) {
  if (ts.isArrayLiteralExpression(node)) return true;
  let found = false;
  ts.forEachChild(node, (child) => {
    if (!found && containsArrayLiteral(child)) found = true;
  });
  return found;
}

function hasTimingLiteral(node) {
  if (
    ts.isPropertyAssignment(node) &&
    ['refetchInterval', 'retryDelay', 'staleTime'].includes(propertyName(node.name))
  ) {
    return positiveNumericLiteral(node.initializer);
  }
  return (
    ts.isCallExpression(node) &&
    ['setTimeout', 'setInterval'].includes(expressionName(node.expression)) &&
    node.arguments.length > 1 &&
    positiveNumericLiteral(node.arguments[1])
  );
}

function expressionName(node) {
  if (ts.isIdentifier(node)) return node.text;
  if (ts.isPropertyAccessExpression(node)) return node.name.text;
  return null;
}

function positiveNumericLiteral(node) {
  return ts.isNumericLiteral(node) && Number(node.text.replaceAll('_', '')) > 0;
}

function isExternalJsxResource(node) {
  if (!ts.isJsxOpeningElement(node) && !ts.isJsxSelfClosingElement(node)) return false;
  if (node.tagName.getText() !== 'link') return false;
  return node.attributes.properties.some(
    (attribute) =>
      ts.isJsxAttribute(attribute) &&
      ['href', 'src'].includes(attribute.name.text) &&
      attribute.initializer !== undefined &&
      ts.isStringLiteral(attribute.initializer) &&
      /^(?:https?:)?\/\//iu.test(attribute.initializer.text),
  );
}

function featureName(relativePath) {
  return relativePath.match(/^src\/features\/([^/]+)\//u)?.[1] ?? null;
}

function importedFeature(relativePath, specifier) {
  if (!specifier.startsWith('.')) return null;
  const resolved = path.posix.normalize(
    path.posix.join(path.posix.dirname(relativePath), specifier),
  );
  return resolved.match(/(?:^|\/)src\/features\/([^/]+)(?:\/|$)/u)?.[1] ?? null;
}

function packageName(specifier) {
  if (!specifier.startsWith('@')) return specifier.split('/')[0] ?? specifier;
  return specifier.split('/').slice(0, 2).join('/');
}

function isTestFile(relativePath, policy) {
  return (
    /(?:^|\/)tests?\//u.test(relativePath) ||
    /\.(?:spec|test)\.[^.]+$/u.test(relativePath) ||
    policy.testSupportFiles.includes(relativePath)
  );
}

async function walk(root, { missingIsEmpty = false } = {}) {
  let entries;
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch (error) {
    if (missingIsEmpty && error?.code === 'ENOENT') return [];
    throw error;
  }
  const files = [];
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const target = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...(await walk(target)));
    else if (entry.isFile()) files.push(target);
  }
  return files;
}

async function readJson(file) {
  return JSON.parse(await readFile(file, 'utf8'));
}

function normalizePath(value) {
  return value.split(path.sep).join('/');
}

async function main() {
  const distFlag = process.argv.indexOf('--dist');
  const distDirectory = distFlag === -1 ? null : process.argv[distFlag + 1];
  if (distFlag !== -1 && (distDirectory === undefined || distDirectory.startsWith('--'))) {
    throw new Error('--dist requires a directory');
  }
  const violations = await verifyArchitecture(DEFAULT_ROOT, { distDirectory });
  if (violations.length > 0) {
    for (const violation of violations) console.error(`- ${violation}`);
    console.error(`frontend architecture check rejected ${violations.length} violation(s)`);
    process.exitCode = 1;
    return;
  }
  console.log(
    `frontend architecture check passed${distDirectory === null ? '' : ' with bundle budgets'}`,
  );
}

if (path.resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  await main();
}
