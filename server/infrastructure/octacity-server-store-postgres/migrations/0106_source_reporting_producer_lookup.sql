-- Reporter review resolves original Work and its partition-qualified canonical ticket.
-- Keep these point reads independent of Project preparation/link history size.
CREATE INDEX factory_source_preparations_work
  ON factory_source_preparations(project_id, work_id) WHERE settled;
CREATE INDEX issue_source_links_native_correlation
  ON issue_source_links(project_id, native_correlation);
