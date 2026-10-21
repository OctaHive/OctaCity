/** Result continuation and detail selection are valid only for their original execution. */
export function selectIssueExecution(params: URLSearchParams, requestId: string) {
  const next = new URLSearchParams(params);
  if (next.get('execution') !== requestId) {
    next.delete('result_after');
    next.delete('result_id');
    next.delete('check_record');
    next.delete('check_after');
    next.delete('candidate_after');
    next.delete('artifact_after');
    next.delete('journal_build');
    next.delete('journal_job');
    next.delete('journal_event');
  }
  next.set('execution', requestId);
  return next;
}
