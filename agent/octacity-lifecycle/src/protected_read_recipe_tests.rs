mod protected_read_recipe_tests {
  use super::*;
  use crate::ProtectedReadRecipe;

  #[test]
  fn an_installed_read_recipe_has_exact_arguments_and_only_one_read_effect() {
    let expected = protected_read_dispatcher_tests::read_proposal();
    let recipe = ProtectedReadRecipe::new(
      expected.tool.clone(),
      expected.executable.clone(),
      expected.resources,
      expected.outputs.clone(),
    )
    .unwrap();
    assert_eq!(
      recipe.action("/workspace/source/input").unwrap().into_proposal(),
      expected
    );
    assert!(recipe.action("/workspace/source/../private").is_err());
    assert!(recipe.action("relative").is_err());
  }
}
