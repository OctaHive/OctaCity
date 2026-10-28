// Bounded finite peer. Optional read credentials exercise the same native MCP path.
import { readFileSync, appendFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
if (process.env.OPENAI_API_KEY !== undefined) throw new Error('unexpected credential');
const scoped = process.env.PEER_RESOURCE !== undefined;
if (scoped && ['HTTP_READ_TOKEN', 'REPORTER_TOKEN', 'UNSELECTED_TOKEN'].some(name => process.env[name] !== undefined)) throw new Error('unexpected credential');
const readCredential = process.env.STDIO_READ_TOKEN === 'fixture-stdio-read-alpha';
for await (const line of createInterface({ input: process.stdin })) {
  if (Buffer.byteLength(line) > 65536) throw new Error('oversized request');
  const request = JSON.parse(line);
  if (!Object.hasOwn(request, 'id')) continue;
  let result;
  if (request.method === 'initialize') result = { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'retained-facts', version: '1' } };
  // In credential mode even 'write' claims to be read-only: the credential,
  // not this untrusted annotation, must still reject the actual write.
  else if (request.method === 'tools/list') result = { tools: (scoped ? ['facts', 'write'] : ['facts']).map(name => ({ name, ...(scoped ? { annotations: { readOnlyHint: true } } : {}), description: 'Finite retained facts operation', inputSchema: { type: 'object', properties: scoped ? { resource: { type: 'string' } } : {}, additionalProperties: false } })) };
  else if (request.method === 'tools/call' && scoped) {
    const allowed = readCredential && request.params.name === 'facts' && request.params.arguments?.resource === 'alpha';
    if (process.env.PEER_AUDIT_FILE) appendFileSync(process.env.PEER_AUDIT_FILE, JSON.stringify({ tool: request.params.name, resource: request.params.arguments?.resource, allowed }) + '\n');
    result = { content: [{ type: 'text', text: allowed ? 'retained alpha facts' : 'credential does not permit this operation' }], isError: !allowed };
  }
  else if (request.method === 'tools/call' && request.params.name === 'facts') result = { content: [{ type: 'text', text: process.env.LANGUAGE + ':' + readFileSync(new URL('supporting.txt', import.meta.url), 'utf8') }], isError: false };
  else throw new Error('unsupported request');
  console.log(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }));
}
