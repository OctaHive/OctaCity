-- Named context documents use the existing Project input publication and cleanup journal.
-- They are neither task/acceptance pairs nor synthetic Work or Build outputs.
ALTER TABLE project_text_input_objects
  DROP CONSTRAINT project_text_input_objects_role_check;
ALTER TABLE project_text_input_objects
  ADD CONSTRAINT project_text_input_objects_role_check
  CHECK (role IN ('task', 'acceptance', 'document'));
