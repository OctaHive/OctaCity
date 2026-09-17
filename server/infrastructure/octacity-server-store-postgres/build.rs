fn main() {
  // SQLx embeds migration files in the library. Cargo otherwise notices edits
  // to known files but not necessarily a newly added migration.
  println!("cargo::rerun-if-changed=migrations");
}
