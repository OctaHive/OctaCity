use super::*;
use crate::{WorkSourceGroup, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceSelection};

fn inputs(project: ProjectId, provider: &str) -> Vec<WorkSourceInput> {
  let repository = repository(project);
  ["first-selector", "second-selector"]
    .into_iter()
    .map(|selector| {
      let target = target(project, true);
      let mut source = source(project, provider);
      source.binding.configuration = reference(selector);
      let mut definition = draft(&target, &repository, &source);
      definition.enabled = true;
      WorkSourceInput::publish(
        WorkSourceInputId::generate(),
        project,
        definition,
        WorkSourceInputContext {
          target: &target,
          repository: &repository,
          source: Some(&source),
          reporter: None,
        },
      )
      .unwrap()
    })
    .collect()
}

#[test]
fn an_exclusive_group_with_no_matches_filters_the_candidate() {
  let project = ProjectId::generate();
  let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, inputs(project, "github")).unwrap();
  let pass = group.select(&["approved", "bug", "feature-request"], |_, _| false);
  assert_eq!(pass.finish(&group).unwrap(), WorkSourceSelection::Filtered);
}

#[test]
fn one_match_selects_its_exact_input_independently_in_each_group_for_any_provider() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    let project = ProjectId::generate();
    let members = inputs(project, provider);
    // Separate groups deliberately share the same observation and input identity.
    let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, members.clone()).unwrap();
    let independent = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, members.clone()).unwrap();
    for labels in [
      vec!["ready-for-review"],
      vec!["approved", "bug"],
      vec!["Ожидает уточнения"],
      vec![],
    ] {
      for selected_group in [&group, &independent] {
        let pass = selected_group.select(&labels, |member, _| member.id() == members[1].id());
        assert_eq!(
          pass.finish(selected_group).unwrap(),
          WorkSourceSelection::Selected(Box::new(members[1].clone())),
          "{provider}: {labels:?}"
        );
      }
    }
  }
}

#[test]
fn multiple_matches_are_ambiguous_without_affecting_an_independent_group() {
  let project = ProjectId::generate();
  let members = inputs(project, "gitlab");
  let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, members.clone()).unwrap();
  let independent = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, vec![members[1].clone()]).unwrap();
  let candidate = ["documentation", "team:review"];
  assert_eq!(
    group.select(&candidate, |_, _| true).finish(&group).unwrap(),
    WorkSourceSelection::Ambiguous(vec![members[0].id(), members[1].id()])
  );
  assert_eq!(
    independent
      .select(&candidate, |_, _| true)
      .finish(&independent)
      .unwrap(),
    WorkSourceSelection::Selected(Box::new(members[1].clone()))
  );
  // A later corrected observation may select a single input; ambiguity reserves nothing.
  assert_eq!(
    group
      .select(&["documentation"], |member, _| member.id() == members[0].id())
      .finish(&group)
      .unwrap(),
    WorkSourceSelection::Selected(Box::new(members[0].clone()))
  );
}

fn replacement(input: &WorkSourceInput, enabled: bool) -> WorkSourceInput {
  replacement_with_binding(
    input,
    enabled,
    WorkSourceBinding {
      configuration: reference("replacement-selector"),
      ..input.definition().source.clone()
    },
  )
}

fn replacement_with_binding(input: &WorkSourceInput, enabled: bool, binding: WorkSourceBinding) -> WorkSourceInput {
  let target = target_version(
    input.project_id(),
    true,
    input.definition().target.id(),
    FactoryConfigurationVersion::new(2).unwrap(),
  );
  let source = WorkSourceCapability {
    project_id: input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding,
  };
  let mut definition = draft(&target, &input.definition().repository, &source);
  definition.enabled = enabled;
  input
    .replace(
      crate::WorkSourceInputVersion::new(2).unwrap(),
      definition,
      WorkSourceInputContext {
        target: &target,
        repository: &input.definition().repository,
        source: Some(&source),
        reporter: None,
      },
    )
    .unwrap()
}

#[test]
fn publication_rejects_unbounded_duplicate_foreign_or_incompatible_members() {
  let project = ProjectId::generate();
  let members = inputs(project, "github");
  let bounded: Vec<_> = (0..32).flat_map(|_| inputs(project, "github")).collect();
  assert_eq!(bounded.len(), 64);
  let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, bounded.clone()).unwrap();
  assert_eq!(group.members().len(), 64);
  let mut oversized = bounded;
  oversized.push(members[0].clone());
  let mut invalid = vec![
    (vec![], "source group members"),
    (oversized, "source group members"),
    (
      vec![members[0].clone(), members[0].clone()],
      "source group duplicate member",
    ),
    (
      vec![members[0].clone(), replacement(&members[0], true)],
      "source group duplicate member",
    ),
    (inputs(ProjectId::generate(), "github"), "source group member project"),
  ];
  for field in ["installation", "capability", "resource"] {
    let mut binding = members[1].definition().source.clone();
    match field {
      "installation" => binding.installation = reference("another-installation"),
      "capability" => binding.capability = reference("another-capability"),
      "resource" => binding.resource = reference("another-ticket-container"),
      _ => unreachable!(),
    }
    invalid.push((
      vec![members[0].clone(), replacement_with_binding(&members[1], true, binding)],
      "source group observation scope",
    ));
  }
  for (submitted, field) in invalid {
    let expected = crate::FactoryError::InvalidConfiguration { field };
    assert_eq!(
      WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, submitted.clone()).unwrap_err(),
      expected
    );
    assert_eq!(
      group
        .replace(WorkSourceGroupVersion::new(2).unwrap(), submitted)
        .unwrap_err(),
      expected
    );
  }
}

#[test]
fn a_group_revision_changing_during_a_pass_invalidates_every_old_disposition() {
  let project = ProjectId::generate();
  let members = inputs(project, "jira");
  let group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, members.clone()).unwrap();
  let new_members: Vec<_> = members.iter().map(|input| replacement(input, true)).collect();
  let retained = serde_json::to_value(&group).unwrap();
  // The native tracker snapshot is opaque to the group (e.g. Jira status/field identifiers).
  let observation = (17_u64, 93_u64);
  for match_count in [0, 1, 2] {
    let mut current = group.clone();
    let mut visited = Vec::new();
    let pass = group.select(&observation, |member, snapshot| {
      current = group
        .replace(WorkSourceGroupVersion::new(2).unwrap(), new_members.clone())
        .unwrap();
      visited.push((member.id(), member.version().get(), *snapshot));
      visited.len() <= match_count
    });
    assert_eq!(
      visited,
      vec![(members[0].id(), 1, (17, 93)), (members[1].id(), 1, (17, 93))]
    );
    assert_eq!(
      pass.finish(&current).unwrap_err(),
      crate::FactoryError::InvalidConfiguration {
        field: "source group current version"
      }
    );
    assert_eq!(current.id(), group.id());
    assert_eq!(current.project_id(), project);
    assert_eq!(current.version().get(), 2);
    assert_eq!(current.members(), new_members.as_slice());
    assert_eq!(
      current
        .select(&observation, |member, _| member.id() == members[1].id())
        .finish(&current)
        .unwrap(),
      WorkSourceSelection::Selected(Box::new(new_members[1].clone()))
    );
  }
  assert_eq!(serde_json::to_value(&group).unwrap(), retained);
  assert_eq!(group.version().get(), 1);
  for version in [1, 3] {
    assert_eq!(
      group
        .replace(WorkSourceGroupVersion::new(version).unwrap(), new_members.clone())
        .unwrap_err(),
      crate::FactoryError::InvalidConfiguration {
        field: "source group replacement version"
      }
    );
  }
  // Reconstructed metadata cannot substitute different members under the same id/version.
  let substituted = WorkSourceGroup::publish(group.id(), project, new_members).unwrap();
  assert!(group.select(&observation, |_, _| true).finish(&substituted).is_err());
}

#[test]
fn disabled_members_cannot_match_or_create_ambiguity() {
  let project = ProjectId::generate();
  let members = inputs(project, "gitlab");
  let disabled = replacement(&members[0], false);
  let group = WorkSourceGroup::publish(
    WorkSourceGroupId::generate(),
    project,
    vec![disabled.clone(), members[1].clone()],
  )
  .unwrap();
  let mut evaluated = Vec::new();
  let result = group
    .select(&(), |member, _| {
      evaluated.push(member.id());
      true
    })
    .finish(&group)
    .unwrap();
  assert_eq!(result, WorkSourceSelection::Selected(Box::new(members[1].clone())));
  assert_eq!(evaluated, vec![members[1].id()]);

  let disabled_group = WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, vec![disabled]).unwrap();
  assert_eq!(
    disabled_group
      .select(&(), |_, _| panic!("disabled input must not be evaluated"))
      .finish(&disabled_group)
      .unwrap(),
    WorkSourceSelection::Filtered
  );
}
