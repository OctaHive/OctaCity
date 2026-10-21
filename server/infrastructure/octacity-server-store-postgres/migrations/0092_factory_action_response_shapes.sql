-- Older derived rows retain outcome references but not the pinned response shapes
-- needed by Console. Authority remains in the original Run/Flow/control records.
-- Publish explicit incomplete coverage until the existing resumable worker rebuilds
-- these caches; never reinterpret an old row or repair it during a GET.
DELETE FROM factory_action_items;
DELETE FROM factory_action_projections;
