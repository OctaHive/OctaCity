/// Produces a non-empty, control-free diagnostic within an exact UTF-8 byte limit.
pub(crate) fn bounded_diagnostic(value: &str, maximum_bytes: usize, fallback: &str) -> String {
  let mut diagnostic = String::with_capacity(value.len().min(maximum_bytes));
  for character in value.chars() {
    if character.is_control() {
      continue;
    }
    if diagnostic.len() + character.len_utf8() > maximum_bytes {
      break;
    }
    diagnostic.push(character);
  }
  if diagnostic.is_empty() {
    fallback.to_owned()
  } else {
    diagnostic
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn output_is_control_free_and_utf8_boundary_safe() {
    let input = format!("{}\nsecret", "Ж".repeat(4_096));
    let diagnostic = bounded_diagnostic(&input, 4_096, "failed");

    assert!(diagnostic.len() <= 4_096);
    assert!(!diagnostic.chars().any(char::is_control));
    assert!(diagnostic.is_char_boundary(diagnostic.len()));
  }
}
