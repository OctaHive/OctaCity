//! Role authoring reuses the ordinary immutable references, permissions, budgets and Flow grammar.
use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let b = octacity_server_application::factory_configuration_document_bounds();
  let uuid = || json!({"type":"string","format":"uuid"});
  let text = || json!({"type":"string","maxLength":b.text_bytes});
  let reference = || schema_ref("FactoryImmutableReferenceResource");
  let digest = || schema_ref("FactoryDigest");
  schemas.insert(
    "ProjectSkillDetails".into(),
    object(
      [
        (
          "bundle",
          object(
            [
              ("reference", reference()),
              (
                "files",
                json!({"type":"object", "additionalProperties":schema_ref("FactoryArtifactReference"),
                  "maxProperties":b.skill_files, "propertyNames":{"type":"string", "minLength":1, "maxLength":b.skill_path_bytes}}),
              ),
            ],
            &["reference", "files"],
          ),
        ),
        (
          "file",
          nullable(object(
            [
              ("path", non_empty_string()),
              ("size_bytes", non_negative_integer()),
              (
                "text",
                nullable(
                  json!({"type":"string", "maxLength":octacity_server_application::MAX_PROJECT_SKILL_PREVIEW_BYTES}),
                ),
              ),
            ],
            &["path", "size_bytes", "text"],
          )),
        ),
      ],
      &["bundle", "file"],
    ),
  );
  let purpose = || json!({"type":"string","enum":["model","evaluator","source","delivery"]});
  let instruction = json!({"type":"string","maxLength":b.instruction_bytes});
  schemas.insert("FactoryInstructions".into(),json!({"oneOf":[instruction,bounded_array(instruction.clone(),b.role_entries)],"x-max-utf8-bytes":b.instruction_bytes}));
  for name in ["FactoryRoleReference", "FactoryRoleProfileReference"] {
    schemas.insert(
      name.into(),
      object(
        [
          ("project_id", uuid()),
          ("id", uuid()),
          ("version", positive_integer()),
          ("digest", digest()),
        ],
        &["project_id", "id", "version", "digest"],
      ),
    );
  }
  let mut role = object(
    [
      ("template", nullable(reference())),
      ("name", text()),
      ("instructions", schema_ref("FactoryInstructions")),
      ("skills", bounded_map(reference(), b.role_entries)),
      ("icon", nullable(key())),
      (
        "avatar",
        nullable(json!({"type":"string", "maxLength":b.role_avatar_bytes,
        "pattern":"^data:image/png;base64,[A-Za-z0-9+/]+={0,2}$"})),
      ),
      ("description", text()),
      ("concurrency_limit", nullable(bounded_integer(1, u32::MAX.into()))),
    ],
    &["name", "instructions"],
  );
  role["x-max-encoded-bytes"] = json!(b.role_definition_bytes);
  schemas.insert("FactoryRoleDefinition".into(), role);
  schemas.insert(
    "FactoryRole".into(),
    object(
      [
        ("reference", schema_ref("FactoryRoleReference")),
        ("definition", schema_ref("FactoryRoleDefinition")),
      ],
      &["reference", "definition"],
    ),
  );
  schemas.insert(
    "FactoryRoleToolchain".into(),
    object(
      [
        ("plugin", reference()),
        ("executable", reference()),
        ("model", reference()),
      ],
      &["plugin", "executable", "model"],
    ),
  );
  schemas.insert("FactoryRoleEnvironmentValue".into(),json!({"oneOf":[
    tagged("kind","public",[("value",text())], &["value"]),
    tagged("kind","secret",[("reference",object([("provider",non_empty_string()),("reference",non_empty_string())], &["provider","reference"]))], &["reference"])
  ]}));
  schemas.insert(
    "FactoryRoleMcpBinding".into(),
    object(
      [("configuration", reference()), ("integration", nullable(key()))],
      &["configuration"],
    ),
  );
  schemas.insert(
    "FactoryRoleIntegrationBinding".into(),
    object(
      [
        ("project_id", uuid()),
        ("connection_id", uuid()),
        ("connection_version", positive_integer()),
        ("access_version", positive_integer()),
        ("resource", reference()),
        ("capability", reference()),
      ],
      &[
        "project_id",
        "connection_id",
        "connection_version",
        "access_version",
        "resource",
        "capability",
      ],
    ),
  );
  schemas.insert("FactoryRoleProfileExecution".into(),object([
    ("environment",json!({"type":"object","maxProperties":b.role_entries,"propertyNames":{"type":"string","maxLength":b.key_bytes,"pattern":"^[A-Za-z_][A-Za-z0-9_]*$"},"additionalProperties":schema_ref("FactoryRoleEnvironmentValue")})),
    ("cli_arguments",bounded_array(json!({"type":"string","maxLength":b.argument_bytes}),b.command_arguments)),
    ("mcp_servers",bounded_map(schema_ref("FactoryRoleMcpBinding"),b.role_entries)),
    ("integrations",bounded_map(schema_ref("FactoryRoleIntegrationBinding"),b.role_entries)),
  ], &[]));
  let mut profile = object(
    [
      ("template", nullable(reference())),
      ("name", text()),
      ("instructions", schema_ref("FactoryInstructions")),
      ("toolchain", schema_ref("FactoryRoleToolchain")),
      ("execution", schema_ref("FactoryRoleProfileExecution")),
      ("thinking_parameter", nullable(key())),
      (
        "model_parameters",
        bounded_map(
          json!({"type":["string","number","boolean","null"],"maxLength":b.text_bytes}),
          b.role_entries,
        ),
      ),
      ("skills", bounded_map(reference(), b.role_entries)),
      ("context_requirements", bounded_array(key(), b.role_entries)),
      (
        "build_configuration",
        schema_ref("FactoryBuildConfigurationReferenceResource"),
      ),
      ("credential_purpose", purpose()),
      ("permissions", schema_ref("FactoryPermissions")),
      ("budget", schema_ref("FactoryBudget")),
    ],
    &[
      "name",
      "instructions",
      "toolchain",
      "model_parameters",
      "skills",
      "context_requirements",
      "build_configuration",
      "credential_purpose",
      "permissions",
      "budget",
    ],
  );
  profile["x-max-encoded-bytes"] = json!(b.role_definition_bytes);
  schemas.insert("FactoryRoleProfileDefinition".into(), profile);
  schemas.insert(
    "FactoryRoleProfile".into(),
    object(
      [
        ("reference", schema_ref("FactoryRoleProfileReference")),
        ("role", schema_ref("FactoryRoleReference")),
        ("definition", schema_ref("FactoryRoleProfileDefinition")),
      ],
      &["reference", "role", "definition"],
    ),
  );
  schemas.insert(
    "FactoryRoleSummary".into(),
    object(
      [
        ("reference", schema_ref("FactoryRoleReference")),
        ("name", text()),
        ("icon", nullable(key())),
        ("concurrency_limit", nullable(positive_integer())),
        ("template", nullable(reference())),
      ],
      &["reference", "name", "icon", "concurrency_limit", "template"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfileSummary".into(),
    object(
      [
        ("reference", schema_ref("FactoryRoleProfileReference")),
        ("role", schema_ref("FactoryRoleReference")),
        ("name", text()),
        ("toolchain", schema_ref("FactoryRoleToolchain")),
        ("template", nullable(reference())),
      ],
      &["reference", "role", "name", "toolchain", "template"],
    ),
  );
  for (name, item) in [
    ("FactoryRolePage", "FactoryRoleSummary"),
    ("FactoryRoleProfilePage", "FactoryRoleProfileSummary"),
    ("FactoryRoleTemplatePage", "FactoryRoleTemplateSummary"),
    ("FactoryRoleProfileChoicePage", "FactoryRoleProfileChoice"),
    ("ProjectSkillCatalogPage", "FactoryImmutableReferenceResource"),
  ] {
    schemas.insert(
      name.into(),
      object(
        [
          ("total", non_negative_integer()),
          (
            "items",
            bounded_array(
              schema_ref(item),
              usize::from(octacity_server_application::MAX_CURRENT_DEFINITION_PAGE_SIZE),
            ),
          ),
          ("next_cursor", nullable(non_empty_string())),
        ],
        &["total", "items", "next_cursor"],
      ),
    );
  }
  schemas.insert(
    "FactoryRolePublication".into(),
    object(
      [
        ("id", uuid()),
        ("expected_current_version", nullable(positive_integer())),
        ("definition", schema_ref("FactoryRoleDefinition")),
      ],
      &["id", "definition"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfilePublication".into(),
    object(
      [
        ("id", uuid()),
        ("expected_current_version", nullable(positive_integer())),
        ("role", schema_ref("FactoryRoleReference")),
        ("definition", schema_ref("FactoryRoleProfileDefinition")),
      ],
      &["id", "role", "definition"],
    ),
  );
  schemas.insert(
    "PublishFactoryRoleProfilesRequest".into(),
    object(
      [
        ("role", nullable(schema_ref("FactoryRolePublication"))),
        (
          "profiles",
          bounded_array(schema_ref("FactoryRoleProfilePublication"), b.role_entries),
        ),
      ],
      &["profiles"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfilesPublication".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("role", nullable(schema_ref("FactoryRole"))),
        (
          "profiles",
          bounded_array(schema_ref("FactoryRoleProfile"), b.role_entries),
        ),
        ("published_at", integer()),
      ],
      &["disposition", "role", "profiles", "published_at"],
    ),
  );
  schemas.insert(
    "ValidateFactoryRoleRequest".into(),
    object(
      [("id", uuid()), ("definition", schema_ref("FactoryRoleDefinition"))],
      &["id", "definition"],
    ),
  );
  schemas.insert(
    "FactoryRoleTemplateSummary".into(),
    object(
      [
        ("reference", reference()),
        ("name", text()),
        ("profiles", bounded_array(key(), b.role_entries)),
      ],
      &["reference", "name", "profiles"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfileTemplate".into(),
    object(
      [("name", text()), ("instructions", schema_ref("FactoryInstructions"))],
      &["name", "instructions"],
    ),
  );
  schemas.insert(
    "FactoryRoleTemplate".into(),
    object(
      [
        ("identity", key()),
        ("version", key()),
        ("role", schema_ref("FactoryRoleDefinition")),
        (
          "profiles",
          bounded_map(schema_ref("FactoryRoleProfileTemplate"), b.role_entries),
        ),
      ],
      &["identity", "version", "role", "profiles"],
    ),
  );
  schemas.insert(
    "CopyFactoryRoleTemplateRequest".into(),
    object(
      [
        ("template", reference()),
        ("role_id", uuid()),
        ("role", schema_ref("FactoryRoleDefinition")),
        (
          "profiles",
          bounded_map(
            object(
              [
                ("id", uuid()),
                ("definition", schema_ref("FactoryRoleProfileDefinition")),
              ],
              &["id", "definition"],
            ),
            b.role_entries,
          ),
        ),
      ],
      &["template", "role_id", "role", "profiles"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfileCapabilityDetails".into(),
    object(
      [
        ("definition", schema_ref("FactoryRoleProfileDefinition")),
        ("parameters", schema_ref("FactoryFlowDataSchema")),
        ("public_environment", environment_schemas(b.role_entries)),
      ],
      &["definition", "parameters", "public_environment"],
    ),
  );
  schemas.insert(
    "FactoryRoleProfileChoice".into(),
    object(
      [
        ("alias", key()),
        ("toolchain", schema_ref("FactoryRoleToolchain")),
        (
          "build_configuration",
          schema_ref("FactoryBuildConfigurationReferenceResource"),
        ),
        ("parameters", schema_ref("FactoryFlowDataSchema")),
        ("public_environment", environment_schemas(b.role_entries)),
        (
          "secret_environment_names",
          bounded_array(non_empty_string(), b.role_entries),
        ),
        ("skills", bounded_map(reference(), b.role_entries)),
        (
          "mcp_servers",
          bounded_map(schema_ref("FactoryRoleMcpBinding"), b.role_entries),
        ),
        (
          "integrations",
          bounded_map(schema_ref("FactoryRoleIntegrationBinding"), b.role_entries),
        ),
        ("credential_purpose", purpose()),
        ("tools", bounded_array(reference(), b.permission_entries)),
        ("budget", schema_ref("FactoryBudget")),
      ],
      &[
        "alias",
        "toolchain",
        "build_configuration",
        "parameters",
        "public_environment",
        "secret_environment_names",
        "skills",
        "mcp_servers",
        "integrations",
        "credential_purpose",
        "tools",
        "budget",
      ],
    ),
  );
  schemas.insert(
    "ValidateFactoryRoleProfileRequest".into(),
    object(
      [
        ("id", uuid()),
        ("role", schema_ref("FactoryRoleReference")),
        ("capability", key()),
        ("definition", schema_ref("FactoryRoleProfileDefinition")),
      ],
      &["id", "role", "capability", "definition"],
    ),
  );
  schemas.insert("FactoryRoleProfileValidation".into(),object([("profile",nullable(schema_ref("FactoryRoleProfile"))),("incompatibility",nullable(json!({"type":"string","enum":["model_parameters","toolchain","environment","arguments","dependencies","credential_purpose","budget","permissions"]})))], &["profile","incompatibility"]));
  let mut import = object(
    [
      ("identity", key()),
      (
        "files",
        bounded_array(
          object(
            [
              (
                "path",
                json!({"type":"string","minLength":1,"maxLength":b.skill_path_bytes}),
              ),
              ("sha256", digest()),
              (
                "bytes",
                json!({"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"maxItems":b.skill_file_bytes}),
              ),
            ],
            &["path", "sha256", "bytes"],
          ),
          b.skill_files,
        ),
      ),
    ],
    &["identity", "files"],
  );
  import["description"] = json!(
    "Explicit regular files only. Common reviewed-tree validation owns paths, hashes and aggregate bytes; the management JSON body limit is 8 MiB including encoding."
  );
  import["x-max-file-bytes"] = json!(b.skill_total_bytes);
  schemas.insert("ImportProjectSkillRequest".into(), import);
  schemas.insert(
    "ProjectSkillImportReceipt".into(),
    object(
      [("bundle", reference()), ("published_at", integer())],
      &["bundle", "published_at"],
    ),
  );
  schemas.insert(
    "SelectFactoryProfileRequest".into(),
    object([("profile", schema_ref("FactoryRoleProfileReference"))], &["profile"]),
  );
  schemas.insert(
    "FactoryProfileAdmissionOverride".into(),
    object(
      [
        (
          "stage",
          object(
            [
              ("root", schema_ref("FactoryFlowDefinitionReference")),
              ("calls", bounded_array(key(), b.depth.into())),
              ("node", key()),
            ],
            &["root", "calls", "node"],
          ),
        ),
        ("profile", schema_ref("FactoryRoleProfileReference")),
      ],
      &["stage", "profile"],
    ),
  );
  selection(schemas);
}

fn environment_schemas(maximum: usize) -> Value {
  json!({"type":"object","maxProperties":maximum,"additionalProperties":schema_ref("FactoryFlowDataSchema")})
}

fn selection(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "FactorySignalConsumptionKind".into(),
    json!({"type":"string","enum":["Shadow","Advisory","BoundedControl","Fallback"]}),
  );
  let b = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert("FactoryProfileSelectionPolicy".into(),json!({"oneOf":[
    tagged("mode","manual",[("candidate",nullable(key()))], &[]),
    tagged("mode","rules",[("rules",bounded_array(object([("when",schema_ref("FactoryFlowPredicate")),("candidate",key())], &["when","candidate"]),b.role_entries)),("otherwise",nullable(key()))], &["rules"]),
    tagged("mode","decision_api",[("policy",schema_ref("FactoryImmutableReferenceResource")),("input",schema_ref("FactoryFlowDataMapping"))], &["policy","input"])
  ]}));
  schemas.insert("FactoryProfileSelectionBinding".into(),object([
    ("role",schema_ref("FactoryRoleReference")),("candidates",bounded_map(object([("profile",schema_ref("FactoryRoleProfile")),("build",schema_ref("FactoryFlowBuildBinding"))], &["profile","build"]),b.role_entries)),
    ("policy",schema_ref("FactoryProfileSelectionPolicy")),("failure",json!({"oneOf":[tagged("kind","manual_selection",[], &[]),tagged("kind","reserve_profile",[("candidate",key())], &["candidate"])]})),("selection_budget",schema_ref("FactoryBudgetUsage"))
  ], &["role","candidates","policy","failure","selection_budget"]));
  schemas.insert("FactoryProfileRuleOutcome".into(),json!({"oneOf":[
    tagged("kind","matched",[("rule",non_negative_integer())], &["rule"]),tagged("kind","catch_all",[], &[]),tagged("kind","no_match",[], &[]),
    tagged("kind","failed",[("reason",json!({"type":"string","enum":["missing_input","invalid_type","evaluation_bound_exceeded"]}))], &["reason"])
  ]}));
  schemas.insert(
    "FactoryProfileBuildProjection".into(),
    object(
      [
        ("operation_id", schema_ref("FactoryDigest")),
        ("node_attempt_id", non_empty_string()),
        ("package_digest", nullable(schema_ref("FactoryDigest"))),
        (
          "executions",
          bounded_array(schema_ref("FactoryBuildExecutionDiagnostic"), b.journal_records),
        ),
      ],
      &["operation_id", "node_attempt_id", "package_digest", "executions"],
    ),
  );
  schemas.insert(
    "FactoryProfileSignalProjection".into(),
    object(
      [
        ("operation_id", non_empty_string()),
        ("receipt_digest", nullable(schema_ref("FactoryDigest"))),
        (
          "state",
          nullable(json!({"type":"string","enum":["completed","timed_out","invalid","unavailable","cancelled"]})),
        ),
        ("consumption", nullable(schema_ref("FactorySignalConsumptionKind"))),
        ("unobservable", boolean()),
      ],
      &["operation_id", "receipt_digest", "state", "consumption", "unobservable"],
    ),
  );
  schemas.insert(
    "FactoryProfileControlProjection".into(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("version", positive_integer()),
        ("profile", schema_ref("FactoryRoleProfileReference")),
        ("recorded_at", integer()),
      ],
      &["id", "version", "profile", "recorded_at"],
    ),
  );
  schemas.insert(
    "FactoryProfileSelectionProjection".into(),
    object(
      [
        ("id", schema_ref("FactoryDigest")),
        ("input_digest", schema_ref("FactoryDigest")),
        ("flow_run_id", non_empty_string()),
        ("cycle_id", non_empty_string()),
        ("node", key()),
        ("generation", non_negative_integer()),
        ("predecessor", nullable(schema_ref("FactoryDigest"))),
        ("control_id", nullable(schema_ref("FactoryDigest"))),
        (
          "method",
          json!({"type":"string","enum":["manual","rules","decision_api"]}),
        ),
        ("policy", nullable(schema_ref("FactoryImmutableReferenceResource"))),
        ("rules", nullable(schema_ref("FactoryProfileRuleOutcome"))),
        ("signal", nullable(schema_ref("FactoryProfileSignalProjection"))),
        ("role", schema_ref("FactoryRoleReference")),
        ("candidate", nullable(key())),
        ("profile", nullable(schema_ref("FactoryRoleProfileReference"))),
        ("runtime", nullable(schema_ref("FactoryImmutableReferenceResource"))),
        ("model", nullable(schema_ref("FactoryImmutableReferenceResource"))),
        (
          "thinking_level",
          nullable(json!({"type":"string","maxLength":b.role_definition_bytes})),
        ),
        (
          "selector_error",
          nullable(json!({"type":"string","enum":["unavailable","invalid_input"]})),
        ),
        (
          "recovery",
          nullable(json!({"type":"string","enum":["reserve_profile","manual_selection","reserve_unavailable"]})),
        ),
        (
          "pending_control",
          nullable(schema_ref("FactoryProfileControlProjection")),
        ),
        ("started_at", integer()),
        ("deadline", integer()),
        ("superseded", boolean()),
        (
          "eligible_candidates",
          bounded_array(
            object(
              [
                ("candidate", key()),
                ("profile", schema_ref("FactoryRoleProfileReference")),
              ],
              &["candidate", "profile"],
            ),
            b.role_entries,
          ),
        ),
        (
          "builds",
          bounded_array(schema_ref("FactoryProfileBuildProjection"), b.journal_records),
        ),
      ],
      &[
        "id",
        "input_digest",
        "flow_run_id",
        "cycle_id",
        "node",
        "generation",
        "predecessor",
        "control_id",
        "method",
        "policy",
        "rules",
        "signal",
        "role",
        "candidate",
        "profile",
        "runtime",
        "model",
        "thinking_level",
        "selector_error",
        "recovery",
        "pending_control",
        "started_at",
        "deadline",
        "superseded",
        "eligible_candidates",
        "builds",
      ],
    ),
  );
  predicates(schemas);
}

fn predicates(schemas: &mut Map<String, Value>) {
  let b = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert("FactoryFlowOperand".into(),json!({"oneOf":[
    tagged("source","observation_time",[], &[]),tagged("source","path",[("path",non_empty_string())], &["path"]),tagged("source","item",[("path",json!({"type":"string"}))], &["path"]),tagged("source","literal",[("value",json!({}))], &["value"])
  ]}));
  schemas.insert("FactoryFlowPredicate".into(),json!({"x-max-depth":b.schema_depth,"oneOf":[
    tagged("operation","absent",[("path",json!({"type":"string"}))], &["path"]),
    tagged("operation","compare",[("left",schema_ref("FactoryFlowOperand")),("comparison",json!({"type":"string","enum":["equal","exact_value","repository_path_within","repository_path_within_any","greater","less_or_equal"]})),("right",schema_ref("FactoryFlowOperand"))], &["left","comparison","right"]),
    tagged("operation","all",[("predicates",bounded_array(schema_ref("FactoryFlowPredicate"),b.schema_entries))], &["predicates"]),
    tagged("operation","any",[("predicates",bounded_array(schema_ref("FactoryFlowPredicate"),b.schema_entries))], &["predicates"]),
    tagged("operation","for_each",[("collection",schema_ref("FactoryFlowOperand")),("quantifier",json!({"type":"string","enum":["any","every"]})),("predicate",schema_ref("FactoryFlowPredicate"))], &["collection","quantifier","predicate"]),
    tagged("operation","not",[("predicate",schema_ref("FactoryFlowPredicate"))], &["predicate"]),tagged("operation","exists",[("operand",schema_ref("FactoryFlowOperand"))], &["operand"])
  ]}));
}
