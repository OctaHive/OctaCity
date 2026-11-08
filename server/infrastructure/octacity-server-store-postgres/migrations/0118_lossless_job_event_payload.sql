-- Process output is bounded JSON, including escaped NUL bytes. JSONB rejects
-- these otherwise valid strings and can block the Agent's durable event queue.
-- No event-payload indexing uses JSONB: retain the original JSON and digest.
ALTER TABLE job_events
  ALTER COLUMN payload TYPE JSON USING payload::JSON;
