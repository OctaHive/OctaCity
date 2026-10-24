// Credential-free, bounded MCP resource used by the released role journey.
import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
if (process.env.OPENAI_API_KEY !== undefined) throw new Error('unexpected credential');
for await (const line of createInterface({ input: process.stdin })) {
  if (Buffer.byteLength(line) > 65536) throw new Error('oversized request');
  const request = JSON.parse(line);
  if (!Object.hasOwn(request, 'id')) continue;
  let result;
  if (request.method === 'initialize') result = { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'retained-facts', version: '1' } };
  else if (request.method === 'tools/list') result = { tools: [{ name: 'facts', description: 'Read retained facts', inputSchema: { type: 'object', properties: {}, additionalProperties: false } }] };
  else if (request.method === 'tools/call' && request.params.name === 'facts') result = { content: [{ type: 'text', text: process.env.LANGUAGE + ':' + readFileSync(new URL('supporting.txt', import.meta.url), 'utf8') }], isError: false };
  else throw new Error('unsupported request');
  console.log(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }));
}
