import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it, vi } from 'vitest';
import { retainJsonEvidence } from '../../../tests/released/retainedEvidence';

it('retains a successful native receipt as a private file independently of reporter buffers', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'octacity-native-evidence-'));
  try {
    const info = {
      outputPath: (...paths: string[]) => join(directory, ...paths),
      attach: vi.fn(async () => {}),
    };
    const receipt = { run: 'verified-run', url: 'https://github.com/example/project/pull/7' };
    await retainJsonEvidence(info, 'confirmed-delivery', receipt);
    const path = join(directory, 'confirmed-delivery.json');
    expect(JSON.parse(await readFile(path, 'utf8'))).toEqual(receipt);
    expect((await stat(path)).mode & 0o777).toBe(0o600);
    expect(info.attach).toHaveBeenCalledWith('confirmed-delivery', {
      path,
      contentType: 'application/json',
    });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
