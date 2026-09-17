-- Storage required by the first complete AuthoritativeStore operations.

ALTER TABLE trigger_occurrences
  ADD COLUMN request_digest BYTEA NOT NULL,
  ADD CONSTRAINT trigger_occurrences_request_digest_length CHECK (octet_length(request_digest) = 32),
  ADD CONSTRAINT trigger_occurrences_deduplication_key
    UNIQUE (trigger_id, trigger_version, deduplication_identity);

ALTER TABLE builds
  ADD COLUMN priority BIGINT NOT NULL DEFAULT 0;
ALTER TABLE builds
  ALTER COLUMN priority DROP DEFAULT;

CREATE SEQUENCE ready_queue_enqueue_order_seq AS BIGINT;
ALTER SEQUENCE ready_queue_enqueue_order_seq OWNED BY ready_queue_entries.enqueue_order;
ALTER TABLE ready_queue_entries
  ALTER COLUMN enqueue_order SET DEFAULT nextval('ready_queue_enqueue_order_seq');

CREATE UNIQUE INDEX ready_queue_enqueue_order_idx
  ON ready_queue_entries (enqueue_order);
CREATE INDEX ready_queue_selection_idx
  ON ready_queue_entries (priority DESC, enqueue_order, job_id);
CREATE INDEX ready_queue_allowed_pools_idx
  ON ready_queue_entries USING GIN (allowed_pool_ids);

ALTER TABLE leases
  ADD CONSTRAINT leases_fence_hash_length CHECK (octet_length(fence_hash) = 32);

ALTER TABLE job_events
  ADD CONSTRAINT job_events_event_digest_length CHECK (octet_length(event_digest) = 32);

CREATE TABLE job_completions (
  job_id UUID PRIMARY KEY REFERENCES jobs(id),
  lease_id UUID NOT NULL UNIQUE REFERENCES leases(id),
  final_sequence BIGINT NOT NULL,
  kind TEXT NOT NULL,
  ready_job_ids UUID[] NOT NULL,
  completed_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT job_completions_nonnegative_sequence CHECK (final_sequence >= 0),
  CONSTRAINT job_completions_kind CHECK (kind IN ('succeeded', 'failed', 'cancelled'))
);
