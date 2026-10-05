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
const generatedConstraints = path.join(generatedDirectory, 'constraints.ts');

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
  const constraints = generateConstraints(document);

  await mkdir(generatedDirectory, { recursive: true });
  await Promise.all([
    writeFile(generatedTypes, declarations, 'utf8'),
    writeFile(generatedConstraints, constraints, 'utf8'),
  ]);
}

function generateConstraints(document) {
  const auditParameters = document.paths?.['/api/v1/audit-facts']?.get?.parameters;
  const attentionParameters = document.paths?.['/api/v1/operator-attention']?.get?.parameters;
  const searchParameters = document.paths?.['/api/v1/search']?.get?.parameters;
  const actorKinds = document.components?.schemas?.AuditActorKind?.enum;
  if (!Array.isArray(auditParameters) || !actorKinds?.every((value) => typeof value === 'string')) {
    throw new Error('Audit query constraints are missing from the management API schema.');
  }
  if (!Array.isArray(searchParameters)) {
    throw new Error('Resource search constraints are missing from the management API schema.');
  }
  if (!Array.isArray(attentionParameters)) {
    throw new Error('Operator attention constraints are missing from the management API schema.');
  }

  const parameter = (parameters, operation, name) => {
    const found = parameters.find(
      (candidate) => candidate.name === name && candidate.in === 'query',
    );
    if (found === undefined) throw new Error(`${operation} query parameter ${name} is missing.`);
    return found;
  };
  const positiveInteger = (value, label) => {
    if (!Number.isSafeInteger(value) || value < 1) {
      throw new Error(`${label} must be a positive integer in the management API schema.`);
    }
    return value;
  };
  const maximumBytes = (parameters, operation, name) =>
    positiveInteger(
      parameter(parameters, operation, name).schema?.['x-max-utf8-bytes'],
      `${operation} ${name} byte limit`,
    );
  const auditLimit = parameter(auditParameters, 'Audit', 'limit').schema;
  const auditValues = {
    actorIdentityMaximumBytes: maximumBytes(auditParameters, 'Audit', 'actor_identity'),
    defaultPageSize: positiveInteger(auditLimit?.default, 'Audit default page size'),
    operationMaximumBytes: maximumBytes(auditParameters, 'Audit', 'operation'),
    requestIdentityMaximumBytes: maximumBytes(auditParameters, 'Audit', 'request_identity'),
    targetIdentityMaximumBytes: maximumBytes(auditParameters, 'Audit', 'target_identity'),
    targetKindMaximumBytes: maximumBytes(auditParameters, 'Audit', 'target_kind'),
  };
  const searchLimit = parameter(searchParameters, 'Resource search', 'limit').schema;
  const searchKinds = parameter(searchParameters, 'Resource search', 'kinds').schema;
  const searchValues = {
    defaultPageSize: positiveInteger(searchLimit?.default, 'Resource search default page size'),
    maximumKinds: positiveInteger(searchKinds?.maxItems, 'Resource search kind limit'),
    maximumPageSize: positiveInteger(searchLimit?.maximum, 'Resource search page size limit'),
    queryMaximumBytes: maximumBytes(searchParameters, 'Resource search', 'query'),
  };
  const attentionLimit = parameter(attentionParameters, 'Operator attention', 'limit').schema;
  const attentionTargets = ['build_ids', 'agent_ids', 'pool_ids'].map(
    (name) => parameter(attentionParameters, 'Operator attention', name).schema?.maxItems,
  );
  const maximumTargets = positiveInteger(attentionTargets[0], 'Operator attention target limit');
  if (!attentionTargets.every((value) => value === maximumTargets)) {
    throw new Error('Operator attention target limits must match in the management API schema.');
  }
  const attentionValues = {
    defaultPageSize: positiveInteger(
      attentionLimit?.default,
      'Operator attention default page size',
    ),
    maximumPageSize: positiveInteger(attentionLimit?.maximum, 'Operator attention page size limit'),
    maximumTargets,
  };

  return `// Generated by scripts/generate-api.mjs. Do not edit.\n\nexport const AUDIT_ACTOR_KINDS = ${JSON.stringify(actorKinds, null, 2)} as const;\n\nexport const AUDIT_QUERY_CONSTRAINTS = ${JSON.stringify(auditValues, null, 2)} as const;\n\nexport const OPERATOR_ATTENTION_CONSTRAINTS = ${JSON.stringify(attentionValues, null, 2)} as const;\n\nexport const RESOURCE_SEARCH_CONSTRAINTS = ${JSON.stringify(searchValues, null, 2)} as const;\n`;
}

try {
  await generateApiTypes();
} catch (error) {
  const details = error instanceof Error ? (error.stack ?? error.message) : String(error);
  process.stderr.write(`Failed to generate management API types.\n${details}\n`);
  process.exitCode = 1;
}
