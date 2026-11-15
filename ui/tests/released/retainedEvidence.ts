import { writeFile } from 'node:fs/promises';
import type { TestInfo } from '@playwright/test';

/** A passing list-reporter run must retain evidence after its in-memory attachments disappear. */
export async function retainJsonEvidence(
  info: Pick<TestInfo, 'outputPath' | 'attach'>,
  name: string,
  value: unknown,
) {
  const path = info.outputPath(`${name}.json`);
  await writeFile(path, JSON.stringify(value), { mode: 0o600 });
  await info.attach(name, { path, contentType: 'application/json' });
}
