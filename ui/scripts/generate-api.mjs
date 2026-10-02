import { execFile } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
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

async function generateApiTypes() {
  const document = await exportOpenApi();
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
