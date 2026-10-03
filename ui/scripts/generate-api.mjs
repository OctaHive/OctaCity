import { execFile } from 'node:child_process';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

import openapiTS, { astToString } from 'openapi-typescript';

const MAX_EXPORTED_OPENAPI_BYTES = 16 * 1024 * 1024;
const execFileAsync = promisify(execFile);
const uiRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(uiRoot, '..');
const generatedDirectory = path.join(uiRoot, '.generated', 'api');
const generatedTypes = path.join(generatedDirectory, 'schema.d.ts');

async function exportOpenApi() {
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--package', 'octacity-server-api-rest', '--example', 'export_openapi'],
    {
      cwd: repositoryRoot,
      encoding: 'utf8',
      maxBuffer: MAX_EXPORTED_OPENAPI_BYTES,
    },
  );

  return JSON.parse(stdout);
}

async function loadOpenApi() {
  const suppliedSchema = process.env.OCTACITY_OPENAPI_SCHEMA_FILE;
  if (suppliedSchema === undefined) {
    return exportOpenApi();
  }
  const schemaPath = path.resolve(uiRoot, suppliedSchema);
  const metadata = await stat(schemaPath);
  if (!metadata.isFile() || metadata.size > MAX_EXPORTED_OPENAPI_BYTES) {
    throw new Error('Supplied management API schema must be a bounded regular file.');
  }
  return JSON.parse(await readFile(schemaPath, 'utf8'));
}

async function generateApiTypes() {
  const document = await loadOpenApi();
  const declarations = astToString(await openapiTS(document));

  await mkdir(generatedDirectory, { recursive: true });
  await writeFile(generatedTypes, declarations, 'utf8');
}

try {
  await generateApiTypes();
} catch (error) {
  const details = error instanceof Error ? (error.stack ?? error.message) : String(error);
  process.stderr.write(`Failed to generate management API types.\n${details}\n`);
  process.exitCode = 1;
}
