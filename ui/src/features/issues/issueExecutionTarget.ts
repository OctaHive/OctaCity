import type { IssueExecutionRequest } from './executionApi';

type Target = IssueExecutionRequest['target'];
/** Compare immutable target identities independently of JSON property ordering. */
export function sameExecutionTarget(left: Target, right: Target) {
  return (
    left.configuration_id === right.configuration_id &&
    left.configuration_version === right.configuration_version &&
    left.repository_id === right.repository_id &&
    left.repository_version === right.repository_version &&
    left.mapping === right.mapping &&
    left.predecessor_run === right.predecessor_run &&
    left.source.kind === right.source.kind &&
    (left.source.kind === 'default_reference' ||
      (right.source.kind !== 'default_reference' && left.source.value === right.source.value))
  );
}
