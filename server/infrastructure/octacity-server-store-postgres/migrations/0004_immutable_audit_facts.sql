CREATE FUNCTION reject_audit_fact_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'audit facts are immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER audit_facts_are_immutable
BEFORE UPDATE OR DELETE ON audit_facts
FOR EACH ROW
EXECUTE FUNCTION reject_audit_fact_mutation();
