import { expect, it } from 'vitest';
import { selectIssueExecution } from './issueExecutionSelection';
it('clears execution-bound journal and result selection only when choosing another execution', () => {
  const params = new URLSearchParams(
    'execution=old&section=runs&result_id=result&result_after=result.tail&journal_build=build&journal_job=job&journal_event=7&journal_collection=node-stops',
  );
  expect(selectIssueExecution(params, 'old').toString()).toBe(params.toString());
  expect([...selectIssueExecution(params, 'new')]).toEqual([
    ['execution', 'new'],
    ['section', 'runs'],
    ['journal_collection', 'node-stops'],
  ]);
});
it('replaces a requestless Work selection when choosing a reviewed execution', () => {
  const next = selectIssueExecution(
    new URLSearchParams('work=original-work&journal_event=7'),
    'reviewed-request',
  );
  expect(next.get('execution')).toBe('reviewed-request');
  expect(next.get('work')).toBeNull();
  expect(next.get('journal_event')).toBeNull();
});
