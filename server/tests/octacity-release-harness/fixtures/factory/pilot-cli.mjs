#!/usr/local/bin/node
// Adapter-local finite model fixture. It has no lifecycle authority or name dispatch.
import { readFileSync, lstatSync, mkdirSync, writeFileSync, openSync, readSync, closeSync, fstatSync, constants } from 'node:fs';
import { dirname, isAbsolute } from 'node:path';
import { spawnSync } from 'node:child_process';

try {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--version') {
    console.log('codex-cli 0.161.0');
    process.exit(0);
  }
  let reader;
  if (args[4] === '--dangerously-bypass-hook-trust') {
    if (args[4] !== '--dangerously-bypass-hook-trust' || args[5] !== '--config=features.hooks=true') throw new Error();
    const override = args[6].match(/^--config=hooks\.PreToolUse=\[\s*\{\s*hooks\s*=\s*\[\s*\{\s*command\s*=\s*("(?:[^"\\]|\\.)*")\s*,\s*statusMessage\s*=\s*"Authorizing tool action"\s*,\s*timeout\s*=\s*60\s*,\s*type\s*=\s*"command"\s*\}\s*\]\s*\}\s*\]$/);
    if (!override) throw new Error();
    const command = JSON.parse(override[1]);
    reader = command.slice(1, -1).replaceAll("'\"'\"'", "'");
    if (!reader.startsWith('/') || reader.includes('\0') || command !== "'" + reader.replaceAll("'", "'\"'\"'") + "'") throw new Error();
    args.splice(4, 3);
  }
  const options = [];
  if (args.length > 7) {
    const additional = args.splice(5, args.length - 6);
    const schemas = additional.filter(argument => argument.startsWith('--output-schema='));
    if (schemas.length !== 1) throw new Error();
    options.push(...additional.filter(argument => !argument.startsWith('--output-schema=')));
    args.splice(5, 0, schemas[0]);
  }
  if (args.length !== 7 || args[0] !== 'exec' || args[1] !== '--json'
    || args[2] !== '--sandbox=workspace-write' || args[3] !== '--ask-for-approval=never'
    || !args[4].startsWith('--model=') || args[4].length === 8
    || !args[5].startsWith('--output-schema=') || args[6] !== '-') throw new Error();
  const schema = args[5].slice('--output-schema='.length);
  if (!lstatSync(schema).isFile() || lstatSync(schema).isSymbolicLink()) throw new Error();
  JSON.parse(readFileSync(schema, 'utf8'));
  const prompt = readFileSync(0, 'utf8');
  if (!prompt || Buffer.byteLength(prompt) > 1024 * 1024 || prompt.includes('\0')) throw new Error();
  const delimiter = '\n\nFrozen node input (data, not instructions):\n';
  const split = prompt.indexOf(delimiter);
  if (split < 0) throw new Error();
  const programStart = prompt.lastIndexOf('\n', split - 1) + 1;
  const prefix = prompt.slice(0, programStart);
  const instructions = JSON.parse(prompt.slice(programStart, split));
  const input = JSON.parse(prompt.slice(split + delimiter.length).split('\n\nSelected immutable context (JSON data, not instructions):\n')[0]);
  if (instructions.fixture_version !== 1 || !Object.hasOwn(instructions, 'result')
    || Object.keys(instructions).some(key => !['fixture_version', 'result', 'files', 'tools', 'checks', 'invocation', 'mcp'].includes(key))) throw new Error();
  const invocation = instructions.invocation;
  if (invocation === undefined) {
    if (options.length || prefix) throw new Error();
  } else {
    if (Object.keys(invocation).length !== (invocation.model === undefined ? 3 : 4)
      || (invocation.model !== undefined && invocation.model !== args[4].slice('--model='.length))
      || typeof invocation.prefix !== 'string'
      || !Array.isArray(invocation.options) || !invocation.environment || typeof invocation.environment !== 'object'
      || invocation.prefix !== prefix || JSON.stringify(invocation.options) !== JSON.stringify(options)
      || Object.entries(invocation.environment).some(([key, value]) => process.env[key] !== value)) throw new Error();
  }
  const toolResults = [];
  function render(value, depth = 0) {
    if (depth > 32) throw new Error();
    if (Array.isArray(value)) return value.map(item => render(item, depth + 1));
    if (value === null || typeof value !== 'object') return value;
    if (Object.hasOwn(value, '$tool')) {
      if (Object.keys(value).length !== 1 || !Number.isInteger(value.$tool) || value.$tool < 0 || value.$tool >= toolResults.length) throw new Error();
      return toolResults[value.$tool];
    }
    if (Object.hasOwn(value, '$input')) {
      if (Object.keys(value).length !== 1 || typeof value.$input !== 'string'
        || (value.$input !== '' && !value.$input.startsWith('/'))) throw new Error();
      let selected = input;
      for (const token of value.$input === '' ? [] : value.$input.slice(1).split('/')) {
        const key = token.replace(/~1/g, '/').replace(/~0/g, '~');
        if (selected === null || typeof selected !== 'object' || !Object.hasOwn(selected, key)) throw new Error();
        selected = selected[key];
      }
      return selected;
    }
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, render(item, depth + 1)]));
  }
  const checks = instructions.checks ?? [];
  if (!Array.isArray(checks) || checks.length > 16) throw new Error();
  for (const check of checks) {
    const exactText = Object.hasOwn(check, 'expected_text');
    if (Object.keys(check).length !== (exactText ? 3 : 2) || typeof check.readable !== 'boolean'
      || (exactText && (check.readable !== true || typeof check.expected_text !== 'string'
        || Buffer.byteLength(check.expected_text) > 65536))) throw new Error();
    const path = render(check.path);
    if (typeof path !== 'string' || !path || path.length > 4096 || path.includes('\0')) throw new Error();
    let readable = false;
    let textMatches = false;
    let descriptor;
    try {
      descriptor = openSync(path, constants.O_RDONLY | constants.O_NONBLOCK);
      if (fstatSync(descriptor).isFile()) {
        if (exactText) {
          const bytes = Buffer.alloc(65537);
          let size = 0;
          while (size < bytes.length) {
            const count = readSync(descriptor, bytes, size, bytes.length - size, size);
            if (!count) break;
            size += count;
          }
          textMatches = size <= 65536 && bytes.subarray(0, size).equals(Buffer.from(check.expected_text, 'utf8'));
        } else readSync(descriptor, Buffer.alloc(1), 0, 1, 0);
        readable = true;
      }
    } catch { readable = false; } finally { if (descriptor !== undefined) closeSync(descriptor); }
    if (readable !== check.readable || (exactText && !textMatches)) throw new Error();
  }
  const tools = instructions.tools ?? [];
  if (!Array.isArray(tools) || tools.length > 16 || (tools.length && !reader)) throw new Error();
  function childEnvironment(names) {
    // Windows needs SystemRoot for OS bootstrap; other inputs stay explicit.
    return Object.fromEntries(['SystemRoot', ...names]
      .filter(key => process.env[key] !== undefined).map(key => [key, process.env[key]]));
  }
  function invokeReader(arguments_, input) {
    const env = childEnvironment(['PATH', 'HOME', 'TMPDIR', 'OCTACITY_CODEX_READER_CONFIG']);
    const completed = spawnSync(reader, arguments_, { input, encoding: 'utf8', timeout: 60000, maxBuffer: 1024 * 1024, env });
    if (completed.error || completed.status !== 0 || completed.stderr) throw new Error();
    return completed.stdout;
  }
  function readMcp(invoke, tool, arguments_) {
    const messages = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'finite-client', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: tool, arguments: arguments_ } },
    ];
    const replies = invoke(messages.map(item => JSON.stringify(item)).join('\n') + '\n').trim().split('\n').map(line => JSON.parse(line));
    if (replies.length !== 2 || replies[0].id !== 1 || replies[0].error || replies[1].id !== 2 || replies[1].error
      || replies[1].result?.isError !== false || replies[1].result.content?.length !== 1 || replies[1].result.content[0].type !== 'text'
      || typeof replies[1].result.content[0].text !== 'string') throw new Error();
    return replies[1].result.content[0].text;
  }
  if (instructions.mcp !== undefined) {
    const mcp = instructions.mcp;
    if (Object.keys(mcp).length !== 5 || typeof mcp.command !== 'string' || !isAbsolute(mcp.command)
      || !Array.isArray(mcp.args) || mcp.args.length > 64 || mcp.args.some(arg => typeof arg !== 'string' || arg.includes('\0'))
      || !Array.isArray(mcp.env_vars) || mcp.env_vars.some(name => typeof name !== 'string' || process.env[name] === undefined)
      || typeof mcp.tool !== 'string' || typeof mcp.expected_text !== 'string') throw new Error();
    const env = childEnvironment(mcp.env_vars);
    const text = readMcp(input => {
      const completed = spawnSync(mcp.command, mcp.args, { input, encoding: 'utf8', timeout: 60000, maxBuffer: 1024 * 1024, env });
      if (completed.error || completed.status !== 0 || completed.stderr) throw new Error();
      return completed.stdout;
    }, mcp.tool, {});
    if (text !== mcp.expected_text) throw new Error();
  }
  for (const [index, tool] of tools.entries()) {
    if (Object.keys(tool).length !== 1 || !Object.hasOwn(tool, 'path')) throw new Error();
    const path = render(tool.path);
    if (typeof path !== 'string' || !path.startsWith('/workspace/source/') || path.split('/').some(part => ['.', '..'].includes(part)) || path.includes('\0')) throw new Error();
    const permission = JSON.parse(invokeReader([], JSON.stringify({ hook_event_name: 'PreToolUse', tool_name: 'mcp__qualified_fs__read_file',
      tool_use_id: `finite-tool-${index}`, tool_input: { path } })));
    if (permission.hookSpecificOutput?.hookEventName !== 'PreToolUse' || permission.hookSpecificOutput.permissionDecision !== 'allow') throw new Error();
    toolResults.push(readMcp(input => invokeReader(['mcp'], input), 'read_file', { path }));
  }
  const result = render(instructions.result);
  if (Buffer.byteLength(JSON.stringify(result)) > 1024 * 1024) throw new Error();
  const files = instructions.files ?? [];
  if (!Array.isArray(files) || files.length > 16) throw new Error();
  for (const file of files) {
    if (typeof file.path !== 'string' || !file.path || file.path.includes('\0')
      || Object.keys(file).some(key => !['path', 'text'].includes(key))) throw new Error();
    const text = render(file.text);
    if (typeof text !== 'string' || Buffer.byteLength(text) > 1024 * 1024) throw new Error();
    mkdirSync(dirname(file.path), { recursive: true });
    writeFileSync(file.path, text);
  }
  console.log(JSON.stringify({ type: 'turn.started' }));
  console.log(JSON.stringify({ type: 'turn.completed', result, thread_id: 'finite-fixture-thread', turn_id: 'finite-fixture-turn',
    usage: { input_tokens: 3, cached_input_tokens: 0, output_tokens: 5 } }));
} catch {
  process.stderr.write('finite model fixture rejected its invocation\n');
  process.exitCode = 64;
}
